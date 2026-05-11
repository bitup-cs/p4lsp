"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const path = __importStar(require("path"));
const fs = __importStar(require("fs"));
const vscode = __importStar(require("vscode"));
const node_1 = require("vscode-languageclient/node");
let client;
let outputChannel;
let reconnectAttempts = 0;
const MAX_RECONNECT_ATTEMPTS = 5;
const RECONNECT_DELAY_MS = 3000;
let reconnectTimeout;
let isDeactivating = false;
function activate(context) {
    outputChannel = vscode.window.createOutputChannel("P4 LSP");
    isDeactivating = false;
    startClient(context);
}
function startClient(context) {
    const config = vscode.workspace.getConfiguration("p4lsp");
    let serverPath = config.get("serverPath");
    // Fallback: bundled binary in extension's server/ folder
    if (!serverPath) {
        const platform = process.platform;
        const arch = process.arch;
        const binName = platform === "win32" ? "p4lsp-server.exe" : "p4lsp-server";
        const bundled = context.asAbsolutePath(path.join("server", binName));
        if (fs.existsSync(bundled)) {
            serverPath = bundled;
        }
        else {
            serverPath = context.asAbsolutePath(path.join("..", "..", "target", "release", binName));
        }
    }
    if (!serverPath || !fs.existsSync(serverPath)) {
        const msg = `P4 Language Server binary not found: ${serverPath}`;
        outputChannel.appendLine(`[p4lsp] ${msg}`);
        vscode.window.showErrorMessage(msg);
        return;
    }
    outputChannel.appendLine(`[p4lsp] Server path: ${serverPath}`);
    const serverOptions = {
        run: { command: serverPath, transport: node_1.TransportKind.stdio },
        debug: { command: serverPath, transport: node_1.TransportKind.stdio },
    };
    const middleware = config.get("enableLogging")
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
    const includePaths = config.get("includePaths") || [];
    const clientOptions = {
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
    client = new node_1.LanguageClient("p4lsp", "P4 Language Server", serverOptions, clientOptions);
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
    }
    catch (err) {
        const msg = `Failed to start P4 Language Server: ${err}`;
        outputChannel.appendLine(`[p4lsp] ${msg}`);
        vscode.window.showErrorMessage(msg);
        scheduleReconnect(context);
    }
}
function scheduleReconnect(context) {
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
function deactivate() {
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
//# sourceMappingURL=extension.js.map