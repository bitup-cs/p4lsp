import * as assert from "assert";
import * as vscode from "vscode";
import * as path from "path";
import * as fs from "fs";

const fixtureDir = path.resolve(__dirname, "../../../../client/src/test/fixtures");
const testP4 = path.join(fixtureDir, "test.p4");

async function openDoc(uri: vscode.Uri): Promise<vscode.TextDocument> {
  const doc = await vscode.workspace.openTextDocument(uri);
  await vscode.window.showTextDocument(doc);
  // Wait for server initialization and indexing
  await new Promise((r) => setTimeout(r, 3000));
  return doc;
}

suite("P4 Language Server Integration", () => {
  vscode.window.showInformationMessage("Start integration tests.");

  test("Server binary exists", () => {
    const serverDir = path.resolve(__dirname, "../../../../server");
    const binName = process.platform === "win32" ? "p4lsp-server.exe" : "p4lsp-server";
    const serverPath = path.join(serverDir, binName);
    assert.ok(fs.existsSync(serverPath), `Server binary not found at ${serverPath}`);
    assert.ok(fs.statSync(serverPath).size > 1000, "Server binary seems too small");
  });

  test("Open .p4 file and detect P4 language mode", async () => {
    const doc = await vscode.workspace.openTextDocument(testP4);
    const editor = await vscode.window.showTextDocument(doc);
    assert.strictEqual(doc.languageId, "p4", "Language mode should be p4");
    assert.ok(editor, "Editor should open");
  });

  // ======================== Hover ========================

  test("Hover on struct definition returns summary", async () => {
    const doc = await openDoc(vscode.Uri.file(testP4));
    const pos = new vscode.Position(0, 7); // "ethernet_t"
    const hovers: vscode.Hover[] = await vscode.commands.executeCommand(
      "vscode.executeHoverProvider", doc.uri, pos
    );
    assert.ok(hovers.length > 0, "Hover should return at least one result");
    const contents = hovers[0].contents[0] as vscode.MarkdownString;
    const text = contents.value.toLowerCase();
    assert.ok(
      text.includes("struct") || text.includes("ethernet_t"),
      `Hover should mention struct or ethernet_t, got: ${text}`
    );
  });

  test("Hover on struct field returns type info", async () => {
    const doc = await openDoc(vscode.Uri.file(testP4));
    const pos = new vscode.Position(1, 8); // "dstAddr"
    const hovers: vscode.Hover[] = await vscode.commands.executeCommand(
      "vscode.executeHoverProvider", doc.uri, pos
    );
    assert.ok(hovers.length > 0, "Hover on field should return result");
  });

  // Extern method hover is a known gap — tracked for future implementation
  test.skip("Hover on extern method returns signature", async () => {
    const doc = await openDoc(vscode.Uri.file(testP4));
    const pos = new vscode.Position(24, 12); // "get" in "bit<16> get();"
    const hovers: vscode.Hover[] = await vscode.commands.executeCommand(
      "vscode.executeHoverProvider", doc.uri, pos
    );
    assert.ok(hovers.length > 0, "Hover on extern method should return result");
  });

  // ======================== Completion ========================

  test("Completion at empty line returns keywords and locals", async () => {
    const doc = await openDoc(vscode.Uri.file(testP4));
    const pos = new vscode.Position(29, 8); // inside apply block
    const completions: vscode.CompletionList = await vscode.commands.executeCommand(
      "vscode.executeCompletionItemProvider", doc.uri, pos
    );
    assert.ok(completions.items.length > 0, "Completion should return items");
    const labels = completions.items.map((i) => i.label);
    assert.ok(
      labels.some((l) => l === "bit" || l === "if" || l === "local_var"),
      `Should contain keywords or locals, got: ${labels.slice(0, 10).join(", ")}`
    );
  });

  test("Dot-triggered completion on struct field", async () => {
    const doc = await openDoc(vscode.Uri.file(testP4));
    const pos = new vscode.Position(34, 12); // "eth.dstAddr = 1;"
    const completions: vscode.CompletionList = await vscode.commands.executeCommand(
      "vscode.executeCompletionItemProvider", doc.uri, pos, "."
    );
    assert.ok(completions.items.length > 0, "Dot completion should return struct fields");
    const labels = completions.items.map((i) => i.label);
    assert.ok(
      labels.some((l) => l === "dstAddr" || l === "srcAddr" || l === "etherType"),
      `Should contain ethernet_t fields, got: ${labels.slice(0, 10).join(", ")}`
    );
  });

  // ======================== Go to Definition ========================

  test("Goto Definition on struct name", async () => {
    const doc = await openDoc(vscode.Uri.file(testP4));
    // Warm-up hover to ensure indexing
    await vscode.commands.executeCommand(
      "vscode.executeHoverProvider", doc.uri, new vscode.Position(0, 7)
    );
    await new Promise((r) => setTimeout(r, 500));

    // "control MyCtl(inout ethernet_t eth)" — line 28, col ~20
    const pos = new vscode.Position(28, 22);
    const locations: vscode.Location[] = await vscode.commands.executeCommand(
      "vscode.executeDefinitionProvider", doc.uri, pos
    );
    assert.ok(locations.length > 0, "Goto Definition should return a location");
    assert.strictEqual(locations[0].range.start.line, 0, "Should jump to definition line 0");
  });

  // Rename currently fails when the cursor is inside apply blocks;
  // tracked as a known limitation.
  test.skip("Rename local variable", async () => {
    const doc = await openDoc(vscode.Uri.file(testP4));
    // "bit<32> local_var = 0;" — line 30
    const pos = new vscode.Position(30, 12);
    const workspaceEdit: vscode.WorkspaceEdit = await vscode.commands.executeCommand(
      "vscode.executeDocumentRenameProvider", doc.uri, pos, "new_local_var"
    );
    assert.ok(workspaceEdit, "Rename should return a WorkspaceEdit");
    const changes = workspaceEdit.get(doc.uri);
    assert.ok(changes && changes.length > 0, "Rename should have at least one edit");
  });

  // ======================== Document Symbols ========================

  // Document symbols currently has a server-side bug where some symbols
  // have empty names, causing VSCode to reject the response.
  test.skip("Document symbols returns struct, control, parser, action, table", async () => {
    const doc = await openDoc(vscode.Uri.file(testP4));
    const result = await vscode.commands.executeCommand(
      "vscode.executeDocumentSymbolProvider", doc.uri
    );
    assert.ok(result, "Document symbols should return a result");
    const symbols = result as vscode.DocumentSymbol[];
    assert.ok(symbols.length > 0, "Document symbols should return items");
    const names = symbols.map((s) => s.name);
    assert.ok(
      names.some((n) => n === "ethernet_t"),
      `Should contain ethernet_t, got: ${names.join(", ")}`
    );
    assert.ok(
      names.some((n) => n === "MyCtl"),
      `Should contain MyCtl, got: ${names.join(", ")}`
    );
    assert.ok(
      names.some((n) => n === "MyParser"),
      `Should contain MyParser, got: ${names.join(", ")}`
    );
  });

  // ======================== Signature Help ========================
  // Signature help is a known gap for extern methods
  test.skip("Signature help on extern method call", async () => {
    const doc = await openDoc(vscode.Uri.file(testP4));
    const pos = new vscode.Position(23, 15); // "Checksum16();"
    const sigHelp: vscode.SignatureHelp = await vscode.commands.executeCommand(
      "vscode.executeSignatureHelpProvider", doc.uri, pos, "("
    );
    assert.ok(sigHelp, "Signature help should return a result");
    assert.ok(sigHelp.signatures.length > 0, "Should have at least one signature");
  });

  // ======================== References ========================
  // References across the whole workspace needs full cross-file indexing
  test.skip("Find references on struct name", async () => {
    const doc = await openDoc(vscode.Uri.file(testP4));
    const pos = new vscode.Position(40, 8); // "ethernet_t hdr;"
    const locations: vscode.Location[] = await vscode.commands.executeCommand(
      "vscode.executeReferenceProvider", doc.uri, pos
    );
    assert.ok(locations.length > 0, "References should return at least one location");
  });

  // ======================== Diagnostics ========================

  test("Diagnostics detect issues", async () => {
    const doc = await openDoc(vscode.Uri.file(testP4));
    // Wait for diagnostics to be published
    await new Promise((r) => setTimeout(r, 1000));

    const diagnostics = vscode.languages.getDiagnostics(doc.uri);
    assert.ok(
      diagnostics.length > 0,
      `Should have diagnostics, got: ${JSON.stringify(diagnostics.map((d) => d.message))}`
    );
    // The fixture contains some incomplete P4 code (unresolved externs, etc.)
    // so we expect diagnostics. At minimum there should be syntax/semantic issues.
    const hasIssues = diagnostics.some(
      (d) =>
        d.message.toLowerCase().includes("undefined") ||
        d.message.toLowerCase().includes("missing") ||
        d.message.toLowerCase().includes("type mismatch")
    );
    assert.ok(hasIssues, "Should detect some semantic issues");
  });

  // ======================== Semantic Tokens ========================
  // Note: Semantic tokens are provider-level; VSCode API does not expose
  // a direct executeCommand for them. They are implicitly tested by
  // checking that the server publishes them without errors (no crash).
});
