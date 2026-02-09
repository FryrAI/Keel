import * as vscode from 'vscode';
import * as cp from 'child_process';
import * as path from 'path';

// --- Status bar ---

let statusBarItem: vscode.StatusBarItem;
let diagnosticCollection: vscode.DiagnosticCollection;

export function activate(context: vscode.ExtensionContext) {
    // Diagnostics
    diagnosticCollection = vscode.languages.createDiagnosticCollection('keel');
    context.subscriptions.push(diagnosticCollection);

    // Status bar
    statusBarItem = vscode.window.createStatusBarItem(
        vscode.StatusBarAlignment.Left,
        100
    );
    statusBarItem.text = '$(shield) keel';
    statusBarItem.tooltip = 'Keel: structural enforcement';
    statusBarItem.command = 'keel.compile';
    statusBarItem.show();
    context.subscriptions.push(statusBarItem);

    // Commands
    context.subscriptions.push(
        vscode.commands.registerCommand('keel.compile', () => runCompile()),
        vscode.commands.registerCommand('keel.discover', () => runDiscover()),
        vscode.commands.registerCommand('keel.where', () => runWhere()),
    );

    // CodeLens provider for function hashes
    const codeLensProvider = new KeelCodeLensProvider();
    context.subscriptions.push(
        vscode.languages.registerCodeLensProvider(
            [
                { language: 'typescript' },
                { language: 'typescriptreact' },
                { language: 'javascript' },
                { language: 'javascriptreact' },
                { language: 'python' },
                { language: 'go' },
                { language: 'rust' },
            ],
            codeLensProvider
        )
    );

    // Auto-compile on save
    context.subscriptions.push(
        vscode.workspace.onDidSaveTextDocument((doc) => {
            const ext = path.extname(doc.fileName);
            if (['.ts', '.tsx', '.js', '.jsx', '.py', '.go', '.rs'].includes(ext)) {
                runCompileFile(doc.uri);
            }
        })
    );
}

export function deactivate() {
    diagnosticCollection?.dispose();
    statusBarItem?.dispose();
}

// --- Keel CLI runner ---

function keelBin(): string {
    return vscode.workspace.getConfiguration('keel').get('binaryPath', 'keel');
}

function workspaceRoot(): string | undefined {
    return vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
}

function runKeelCommand(args: string[]): Promise<string> {
    return new Promise((resolve, reject) => {
        const root = workspaceRoot();
        if (!root) {
            reject(new Error('No workspace folder open'));
            return;
        }
        const proc = cp.execFile(
            keelBin(),
            args,
            { cwd: root, timeout: 30000 },
            (err, stdout, stderr) => {
                if (err && err.code !== 1) {
                    reject(new Error(stderr || err.message));
                    return;
                }
                resolve(stdout);
            }
        );
        proc.on('error', reject);
    });
}

// --- Compile ---

async function runCompile() {
    const root = workspaceRoot();
    if (!root) {
        vscode.window.showWarningMessage('Keel: no workspace folder open');
        return;
    }
    statusBarItem.text = '$(loading~spin) keel compiling...';
    try {
        const output = await runKeelCommand(['compile', '--json']);
        applyDiagnostics(output);
        statusBarItem.text = '$(shield) keel ✓';
    } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : String(e);
        statusBarItem.text = '$(shield) keel ✗';
        vscode.window.showErrorMessage(`Keel compile failed: ${msg}`);
    }
}

async function runCompileFile(uri: vscode.Uri) {
    const root = workspaceRoot();
    if (!root) return;
    const relPath = path.relative(root, uri.fsPath);
    statusBarItem.text = '$(loading~spin) keel...';
    try {
        const output = await runKeelCommand(['compile', '--json', relPath]);
        applyDiagnostics(output);
        statusBarItem.text = '$(shield) keel ✓';
    } catch {
        statusBarItem.text = '$(shield) keel ✗';
    }
}

// --- Discover ---

async function runDiscover() {
    const hash = await vscode.window.showInputBox({
        prompt: 'Enter keel hash to discover',
        placeHolder: 'e.g. 0A1b2C3d4E5',
    });
    if (!hash) return;
    try {
        const output = await runKeelCommand(['discover', '--json', hash]);
        const doc = await vscode.workspace.openTextDocument({
            content: output,
            language: 'json',
        });
        await vscode.window.showTextDocument(doc);
    } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : String(e);
        vscode.window.showErrorMessage(`Keel discover failed: ${msg}`);
    }
}

// --- Where ---

async function runWhere() {
    const hash = await vscode.window.showInputBox({
        prompt: 'Enter keel hash to locate',
        placeHolder: 'e.g. 0A1b2C3d4E5',
    });
    if (!hash) return;
    try {
        const output = await runKeelCommand(['where', '--json', hash]);
        const result = JSON.parse(output);
        if (result.file && result.line) {
            const uri = vscode.Uri.file(
                path.isAbsolute(result.file)
                    ? result.file
                    : path.join(workspaceRoot()!, result.file)
            );
            const doc = await vscode.workspace.openTextDocument(uri);
            const line = Math.max(0, result.line - 1);
            await vscode.window.showTextDocument(doc, {
                selection: new vscode.Range(line, 0, line, 0),
            });
        }
    } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : String(e);
        vscode.window.showErrorMessage(`Keel where failed: ${msg}`);
    }
}

// --- Diagnostics ---

interface CompileOutput {
    errors?: Violation[];
    warnings?: Violation[];
}

interface Violation {
    code: string;
    severity: string;
    message: string;
    file: string;
    line: number;
    fix_hint?: string;
}

function applyDiagnostics(jsonOutput: string) {
    diagnosticCollection.clear();
    if (!jsonOutput.trim()) return; // clean compile = empty stdout

    let result: CompileOutput;
    try {
        result = JSON.parse(jsonOutput);
    } catch {
        return;
    }

    const diagMap = new Map<string, vscode.Diagnostic[]>();

    const addViolation = (v: Violation, severity: vscode.DiagnosticSeverity) => {
        const line = Math.max(0, v.line - 1);
        const range = new vscode.Range(line, 0, line, Number.MAX_SAFE_INTEGER);
        const diag = new vscode.Diagnostic(range, v.message, severity);
        diag.code = v.code;
        diag.source = 'keel';

        const filePath = path.isAbsolute(v.file)
            ? v.file
            : path.join(workspaceRoot()!, v.file);

        const existing = diagMap.get(filePath) || [];
        existing.push(diag);
        diagMap.set(filePath, existing);
    };

    for (const e of result.errors || []) {
        addViolation(e, vscode.DiagnosticSeverity.Error);
    }
    for (const w of result.warnings || []) {
        addViolation(w, vscode.DiagnosticSeverity.Warning);
    }

    for (const [file, diags] of diagMap) {
        diagnosticCollection.set(vscode.Uri.file(file), diags);
    }
}

// --- CodeLens ---

class KeelCodeLensProvider implements vscode.CodeLensProvider {
    provideCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
        const lenses: vscode.CodeLens[] = [];
        const text = document.getText();
        // Match function/class declarations that might have keel hashes in comments
        // Pattern: // keel:HASH or # keel:HASH
        const pattern = /(?:\/\/|#)\s*keel:([A-Za-z0-9]{11})/g;
        let match: RegExpExecArray | null;

        while ((match = pattern.exec(text)) !== null) {
            const pos = document.positionAt(match.index);
            const range = new vscode.Range(pos, pos);
            const hash = match[1];
            lenses.push(
                new vscode.CodeLens(range, {
                    title: `keel: ${hash}`,
                    command: 'keel.discover',
                    arguments: [hash],
                })
            );
        }

        return lenses;
    }
}
