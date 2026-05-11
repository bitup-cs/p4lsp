import * as path from "path";
import * as fs from "fs";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
  Middleware,
} from "vscode-languageclient/node";

let client: LanguageClient;
let outputChannel: vscode.OutputChannel;
let reconnectAttempts = 0;
const MAX_RECONNECT_ATTEMPTS = 5;
const RECONNECT_DELAY_MS = 3000;
let reconnectTimeout: NodeJS.Timeout | undefined;
let isDeactivating = false;

export function activate(context: vscode.ExtensionContext) {
  outputChannel = vscode.window.createOutputChannel("P4 LSP");
  isDeactivating = false;
  startClient(context);
}

function startClient(context: vscode.ExtensionContext) {
  const config = vscode.workspace.getConfiguration("p4lsp");
  let serverPath = config.get<string | null>("serverPath");

  // Fallback: bundled binary in extension's server/ folder
  if (!serverPath) {
    const platform = process.platform;
    const arch = process.arch;
    const binName = platform === "win32" ? "p4lsp-server.exe" : "p4lsp-server";
    const bundled = context.asAbsolutePath(path.join("server", binName));
    if (fs.existsSync(bundled)) {
      serverPath = bundled;
    } else {
      serverPath = context.asAbsolutePath(
        path.join("..", "..", "target", "release", binName)
      );
    }
  }

  if (!serverPath || !fs.existsSync(serverPath)) {
    const msg = `P4 Language Server binary not found: ${serverPath}`;
    outputChannel.appendLine(`[p4lsp] ${msg}`);
    vscode.window.showErrorMessage(msg);
    return;
  }

  outputChannel.appendLine(`[p4lsp] Server path: ${serverPath}`);

  const serverOptions: ServerOptions = {
    run: { command: serverPath, transport: TransportKind.stdio },
    debug: { command: serverPath, transport: TransportKind.stdio },
  };

  const middleware: Middleware | undefined = config.get<boolean>("enableLogging")
    ? {
        provideCompletionItem: async (document, position, context, token, next) => {
          const start = Date.now();
          const result = await next(document, position, context, token);
          console.log(`[p4lsp] completion: ${Date.now() - start}ms`);
          return result;
        },
        provideHover: async (document, position, token, next) => {
          const start = Date.now();
          const result = await next(document, position, token);
          console.log(`[p4lsp] hover: ${Date.now() - start}ms`);
          return result;
        },
      }
    : undefined;

  const includePaths = config.get<string[]>("includePaths") || [];

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: "file", language: "p4" },
      { scheme: "untitled", language: "p4" },
    ],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.p4"),
    },
    middleware,
    outputChannel,
    initializationOptions: {
      includePaths,
    },
  };

  client = new LanguageClient("p4lsp", "P4 Language Server", serverOptions, clientOptions);

  client.onDidChangeState((e) => {
    outputChannel.appendLine(`[p4lsp] State: ${e.oldState} → ${e.newState}`);
    if (e.newState === 3 && !isDeactivating) {
      // 3 = Stopped
      outputChannel.appendLine(`[p4lsp] Server stopped, scheduling reconnect...`);
      scheduleReconnect(context);
    }
  });

  try {
    client.start();
    outputChannel.appendLine("[p4lsp] Client started");
    reconnectAttempts = 0; // Reset on successful start
  } catch (err) {
    const msg = `Failed to start P4 Language Server: ${err}`;
    outputChannel.appendLine(`[p4lsp] ${msg}`);
    vscode.window.showErrorMessage(msg);
    scheduleReconnect(context);
  }
}

function scheduleReconnect(context: vscode.ExtensionContext) {
  if (reconnectTimeout) {
    clearTimeout(reconnectTimeout);
  }
  if (reconnectAttempts >= MAX_RECONNECT_ATTEMPTS) {
    outputChannel.appendLine(`[p4lsp] Max reconnect attempts (${MAX_RECONNECT_ATTEMPTS}) reached. Giving up.`);
    return;
  }
  reconnectAttempts++;
  outputChannel.appendLine(`[p4lsp] Reconnect attempt ${reconnectAttempts}/${MAX_RECONNECT_ATTEMPTS} in ${RECONNECT_DELAY_MS}ms...`);
  reconnectTimeout = setTimeout(() => {
    outputChannel.appendLine("[p4lsp] Attempting reconnect...");
    startClient(context);
  }, RECONNECT_DELAY_MS);
}

export function deactivate(): Thenable<void> | undefined {
  isDeactivating = true;
  if (reconnectTimeout) {
    clearTimeout(reconnectTimeout);
    reconnectTimeout = undefined;
  }
  if (!client) {
    return undefined;
  }
  return client.stop();
}
