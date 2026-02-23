import * as vscode from 'vscode';
import * as cp from 'child_process';
import * as path from 'path';
import * as http from 'http';

const DEFAULT_PORT = 4815;
const HEALTH_INTERVAL_MS = 10_000;
const SUPPORTED_EXTENSIONS = ['.ts', '.tsx', '.js', '.jsx', '.py', '.go', '.rs'];
const SUPPORTED_LANGUAGES = [
    { language: 'typescript' },
    { language: 'typescriptreact' },
    { language: 'javascript' },
    { language: 'javascriptreact' },
    { language: 'python' },
    { language: 'go' },
    { language: 'rust' },
];

let statusBarItem: vscode.StatusBarItem;
let diagnosticCollection: vscode.DiagnosticCollection;
let outputChannel: vscode.OutputChannel;
let serverProcess: cp.ChildProcess | undefined;
let healthTimer: ReturnType<typeof setInterval> | undefined;
let serverAvailable = false;

// --- Activation ---

/** Activate the Keel VS Code extension, registering commands, providers, and the background server. */
export function activate(context: vscode.ExtensionContext) {
    outputChannel = vscode.window.createOutputChannel('Keel');
    diagnosticCollection = vscode.languages.createDiagnosticCollection('keel');

    statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
    statusBarItem.command = 'keel.showOutput';
    setStatusUnknown();
    statusBarItem.show();

    context.subscriptions.push(outputChannel, diagnosticCollection, statusBarItem);

    // Commands
    context.subscriptions.push(
        vscode.commands.registerCommand('keel.compile', cmdCompile),
        vscode.commands.registerCommand('keel.discover', cmdDiscover),
        vscode.commands.registerCommand('keel.where', cmdWhere),
        vscode.commands.registerCommand('keel.showMap', cmdShowMap),
        vscode.commands.registerCommand('keel.startServer', cmdStartServer),
        vscode.commands.registerCommand('keel.stopServer', cmdStopServer),
        vscode.commands.registerCommand('keel.showOutput', () => outputChannel.show()),
    );

    // CodeLens
    context.subscriptions.push(
        vscode.languages.registerCodeLensProvider(SUPPORTED_LANGUAGES, new KeelCodeLensProvider()),
    );

    // Hover
    context.subscriptions.push(
        vscode.languages.registerHoverProvider(SUPPORTED_LANGUAGES, new KeelHoverProvider()),
    );

    // Compile on save
    context.subscriptions.push(
        vscode.workspace.onDidSaveTextDocument((doc) => {
            if (!vscode.workspace.getConfiguration('keel').get('compileOnSave', true)) return;
            if (SUPPORTED_EXTENSIONS.includes(path.extname(doc.fileName))) {
                compileFile(doc.uri);
            }
        }),
    );

    // Auto-start server
    if (vscode.workspace.getConfiguration('keel').get('autoStartServer', true)) {
        tryStartServer();
    } else {
        checkHealth();
    }

    // Health polling
    healthTimer = setInterval(checkHealth, HEALTH_INTERVAL_MS);
    context.subscriptions.push({ dispose: () => clearInterval(healthTimer!) });
}

/** Deactivate the extension by stopping health polling and shutting down the server. */
export function deactivate() {
    clearInterval(healthTimer!);
    stopServer();
}

// --- HTTP client ---

function serverPort(): number {
    return vscode.workspace.getConfiguration('keel').get('serverPort', DEFAULT_PORT);
}

function httpGet(urlPath: string): Promise<string> {
    return new Promise((resolve, reject) => {
        const req = http.get(
            { hostname: '127.0.0.1', port: serverPort(), path: urlPath, timeout: 5000 },
            (res) => {
                let data = '';
                res.on('data', (chunk) => (data += chunk));
                res.on('end', () => {
                    if (res.statusCode && res.statusCode >= 200 && res.statusCode < 300) {
                        resolve(data);
                    } else {
                        reject(new Error(`HTTP ${res.statusCode}: ${data}`));
                    }
                });
            },
        );
        req.on('error', reject);
        req.on('timeout', () => { req.destroy(); reject(new Error('timeout')); });
    });
}

function httpPost(urlPath: string, body: string): Promise<string> {
    return new Promise((resolve, reject) => {
        const req = http.request(
            {
                hostname: '127.0.0.1', port: serverPort(), path: urlPath,
                method: 'POST', timeout: 10000,
                headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body) },
            },
            (res) => {
                let data = '';
                res.on('data', (chunk) => (data += chunk));
                res.on('end', () => {
                    if (res.statusCode && res.statusCode >= 200 && res.statusCode < 300) {
                        resolve(data);
                    } else {
                        reject(new Error(`HTTP ${res.statusCode}: ${data}`));
                    }
                });
            },
        );
        req.on('error', reject);
        req.on('timeout', () => { req.destroy(); reject(new Error('timeout')); });
        req.write(body);
        req.end();
    });
}

// --- Status bar states ---

function setStatusClean() {
    statusBarItem.text = '$(shield) keel ✓';
    statusBarItem.backgroundColor = undefined;
    statusBarItem.tooltip = 'Keel: graph is clean';
}

function setStatusWarnings(count: number) {
    statusBarItem.text = `$(shield) keel ⚠ ${count}`;
    statusBarItem.backgroundColor = new vscode.ThemeColor('statusBarItem.warningBackground');
    statusBarItem.tooltip = `Keel: ${count} warning${count !== 1 ? 's' : ''}`;
}

function setStatusErrors(count: number) {
    statusBarItem.text = `$(shield) keel ✗ ${count}`;
    statusBarItem.backgroundColor = new vscode.ThemeColor('statusBarItem.errorBackground');
    statusBarItem.tooltip = `Keel: ${count} error${count !== 1 ? 's' : ''}`;
}

function setStatusUnknown() {
    statusBarItem.text = '$(shield) keel ?';
    statusBarItem.backgroundColor = undefined;
    statusBarItem.tooltip = 'Keel: server not connected';
}

function setStatusCompiling() {
    statusBarItem.text = '$(loading~spin) keel...';
    statusBarItem.tooltip = 'Keel: compiling...';
}

// --- Server lifecycle ---

function keelBin(): string {
    return vscode.workspace.getConfiguration('keel').get('binaryPath', 'keel');
}

function workspaceRoot(): string | undefined {
    return vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
}

async function checkHealth(): Promise<boolean> {
    try {
        await httpGet('/health');
        if (!serverAvailable) {
            serverAvailable = true;
            outputChannel.appendLine('[keel] Server connected');
        }
        return true;
    } catch {
        if (serverAvailable) {
            serverAvailable = false;
            setStatusUnknown();
            outputChannel.appendLine('[keel] Server unreachable');
        }
        return false;
    }
}

function tryStartServer() {
    const root = workspaceRoot();
    if (!root) { setStatusUnknown(); return; }

    checkHealth().then((alive) => {
        if (alive) return;
        startServer(root);
    });
}

function startServer(cwd: string) {
    outputChannel.appendLine(`[keel] Starting server: ${keelBin()} serve --http --watch`);
    serverProcess = cp.spawn(keelBin(), ['serve', '--http', '--watch'], {
        cwd,
        stdio: ['ignore', 'pipe', 'pipe'],
        detached: false,
    });

    serverProcess.stdout?.on('data', (d) => outputChannel.append(d.toString()));
    serverProcess.stderr?.on('data', (d) => outputChannel.append(d.toString()));
    serverProcess.on('exit', (code) => {
        outputChannel.appendLine(`[keel] Server exited (code ${code})`);
        serverProcess = undefined;
        serverAvailable = false;
        setStatusUnknown();
    });

    // Give it a moment then check health
    setTimeout(checkHealth, 2000);
}

function stopServer() {
    if (!serverProcess) return;
    outputChannel.appendLine('[keel] Stopping server');
    serverProcess.kill('SIGTERM');
    serverProcess = undefined;
    serverAvailable = false;
    setStatusUnknown();
}

// --- Commands ---

async function cmdCompile() {
    const root = workspaceRoot();
    if (!root) { vscode.window.showWarningMessage('Keel: no workspace folder open'); return; }
    if (!serverAvailable) { vscode.window.showWarningMessage('Keel: server not running'); return; }

    setStatusCompiling();
    try {
        const body = JSON.stringify({ path: root });
        const output = await httpPost('/compile', body);
        applyCompileResult(output);
    } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : String(e);
        setStatusErrors(0);
        vscode.window.showErrorMessage(`Keel compile failed: ${msg}`);
    }
}

async function compileFile(uri: vscode.Uri) {
    if (!serverAvailable) return;
    const root = workspaceRoot();
    if (!root) return;

    setStatusCompiling();
    try {
        const relPath = path.relative(root, uri.fsPath);
        const body = JSON.stringify({ path: relPath });
        const output = await httpPost('/compile', body);
        applyCompileResult(output);
    } catch {
        setStatusErrors(0);
    }
}

async function cmdDiscover() {
    if (!serverAvailable) { vscode.window.showWarningMessage('Keel: server not running'); return; }

    const hash = await getHashAtCursor() ?? await vscode.window.showInputBox({
        prompt: 'Enter keel hash to discover',
        placeHolder: 'e.g. 0A1b2C3d4E5',
    });
    if (!hash) return;

    try {
        const output = await httpGet(`/discover/${encodeURIComponent(hash)}`);
        const doc = await vscode.workspace.openTextDocument({ content: output, language: 'json' });
        await vscode.window.showTextDocument(doc, { preview: true });
    } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : String(e);
        vscode.window.showErrorMessage(`Keel discover failed: ${msg}`);
    }
}

async function cmdWhere() {
    if (!serverAvailable) { vscode.window.showWarningMessage('Keel: server not running'); return; }

    const hash = await vscode.window.showInputBox({
        prompt: 'Enter keel hash to locate',
        placeHolder: 'e.g. 0A1b2C3d4E5',
    });
    if (!hash) return;

    try {
        const output = await httpGet(`/where/${encodeURIComponent(hash)}`);
        const result = JSON.parse(output);
        if (result.file && result.line) {
            const filePath = path.isAbsolute(result.file)
                ? result.file
                : path.join(workspaceRoot()!, result.file);
            const doc = await vscode.workspace.openTextDocument(vscode.Uri.file(filePath));
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

async function cmdShowMap() {
    if (!serverAvailable) { vscode.window.showWarningMessage('Keel: server not running'); return; }

    try {
        const output = await httpGet('/map?format=llm');
        const doc = await vscode.workspace.openTextDocument({ content: output, language: 'markdown' });
        await vscode.window.showTextDocument(doc, { preview: true });
    } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : String(e);
        vscode.window.showErrorMessage(`Keel show map failed: ${msg}`);
    }
}

async function cmdStartServer() {
    const root = workspaceRoot();
    if (!root) { vscode.window.showWarningMessage('Keel: no workspace folder open'); return; }
    if (await checkHealth()) { vscode.window.showInformationMessage('Keel: server already running'); return; }
    startServer(root);
}

async function cmdStopServer() {
    if (!serverProcess) {
        vscode.window.showWarningMessage('Keel: no server managed by this extension');
        return;
    }
    stopServer();
    vscode.window.showInformationMessage('Keel: server stopped');
}

// --- Diagnostics ---

interface CompileOutput {
    errors?: Violation[];
    warnings?: Violation[];
}

interface Violation {
    code: string;
    message: string;
    file: string;
    line: number;
    fix_hint?: string;
}

function applyCompileResult(jsonOutput: string) {
    diagnosticCollection.clear();

    if (!jsonOutput.trim()) { setStatusClean(); return; }

    let result: CompileOutput;
    try { result = JSON.parse(jsonOutput); } catch { setStatusClean(); return; }

    const errors = result.errors || [];
    const warnings = result.warnings || [];
    const diagMap = new Map<string, vscode.Diagnostic[]>();

    const addViolation = (v: Violation, severity: vscode.DiagnosticSeverity) => {
        const line = Math.max(0, v.line - 1);
        const range = new vscode.Range(line, 0, line, Number.MAX_SAFE_INTEGER);
        const message = v.fix_hint ? `${v.message}\nFix: ${v.fix_hint}` : v.message;
        const diag = new vscode.Diagnostic(range, message, severity);
        diag.code = v.code;
        diag.source = 'keel';

        const filePath = path.isAbsolute(v.file)
            ? v.file
            : path.join(workspaceRoot()!, v.file);
        const existing = diagMap.get(filePath) || [];
        existing.push(diag);
        diagMap.set(filePath, existing);
    };

    for (const e of errors) addViolation(e, vscode.DiagnosticSeverity.Error);
    for (const w of warnings) addViolation(w, vscode.DiagnosticSeverity.Warning);

    for (const [file, diags] of diagMap) {
        diagnosticCollection.set(vscode.Uri.file(file), diags);
    }

    if (errors.length > 0) {
        setStatusErrors(errors.length);
    } else if (warnings.length > 0) {
        setStatusWarnings(warnings.length);
    } else {
        setStatusClean();
    }

    outputChannel.appendLine(`[keel] Compile: ${errors.length} errors, ${warnings.length} warnings`);
}

// --- CodeLens ---

interface DiscoverResult {
    hash: string;
    name?: string;
    callers?: { name: string; file: string; line: number }[];
    callees?: { name: string; file: string; line: number }[];
    module_context?: string;
}

class KeelCodeLensProvider implements vscode.CodeLensProvider {
    async provideCodeLenses(document: vscode.TextDocument): Promise<vscode.CodeLens[]> {
        if (!serverAvailable) return [];

        const functions = findFunctionDeclarations(document);
        const lenses: vscode.CodeLens[] = [];

        for (const fn of functions) {
            try {
                const output = await httpGet(`/discover/${encodeURIComponent(fn.name)}?file=${encodeURIComponent(fn.relPath)}&line=${fn.line + 1}`);
                const result: DiscoverResult = JSON.parse(output);
                const up = result.callers?.length ?? 0;
                const down = result.callees?.length ?? 0;
                lenses.push(new vscode.CodeLens(fn.range, {
                    title: `↑${up} ↓${down}`,
                    command: 'keel.discover',
                    arguments: [result.hash],
                }));
            } catch {
                // Server unavailable or function not in graph — skip
            }
        }
        return lenses;
    }
}

// --- Hover provider ---

class KeelHoverProvider implements vscode.HoverProvider {
    async provideHover(
        document: vscode.TextDocument,
        position: vscode.Position,
    ): Promise<vscode.Hover | undefined> {
        if (!serverAvailable) return undefined;

        const wordRange = document.getWordRangeAtPosition(position);
        if (!wordRange) return undefined;
        const word = document.getText(wordRange);

        const root = workspaceRoot();
        if (!root) return undefined;
        const relPath = path.relative(root, document.uri.fsPath);

        try {
            const output = await httpGet(
                `/discover/${encodeURIComponent(word)}?file=${encodeURIComponent(relPath)}&line=${position.line + 1}`,
            );
            const result: DiscoverResult = JSON.parse(output);
            const callerCount = result.callers?.length ?? 0;
            const calleeCount = result.callees?.length ?? 0;

            const lines: string[] = [
                `**${result.name ?? word}** \`hash: ${result.hash}\``,
                '',
                `↑${callerCount} caller${callerCount !== 1 ? 's' : ''}${callerCount > 0 ? ': ' + result.callers!.map(c => c.name).join(', ') : ''}`,
                `↓${calleeCount} callee${calleeCount !== 1 ? 's' : ''}${calleeCount > 0 ? ': ' + result.callees!.map(c => c.name).join(', ') : ''}`,
            ];

            if (result.module_context) {
                lines.push('', `Module: ${result.module_context}`);
            }

            return new vscode.Hover(new vscode.MarkdownString(lines.join('\n')), wordRange);
        } catch {
            return undefined;
        }
    }
}

// --- Helpers ---

interface FunctionInfo {
    name: string;
    line: number;
    range: vscode.Range;
    relPath: string;
}

function findFunctionDeclarations(document: vscode.TextDocument): FunctionInfo[] {
    const root = workspaceRoot();
    if (!root) return [];
    const relPath = path.relative(root, document.uri.fsPath);
    const results: FunctionInfo[] = [];

    // Match common function/method/class declarations across supported languages
    const patterns = [
        /^\s*(?:export\s+)?(?:async\s+)?function\s+(\w+)/,       // JS/TS function
        /^\s*(?:export\s+)?(?:const|let)\s+(\w+)\s*=\s*(?:async\s+)?\(/, // JS/TS arrow
        /^\s*(?:export\s+)?class\s+(\w+)/,                        // JS/TS/Python class
        /^\s*(?:async\s+)?def\s+(\w+)/,                           // Python def
        /^\s*func\s+(\w+)/,                                       // Go func
        /^\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)/,                 // Rust fn
    ];

    for (let i = 0; i < document.lineCount; i++) {
        const lineText = document.lineAt(i).text;
        for (const pat of patterns) {
            const m = pat.exec(lineText);
            if (m) {
                results.push({
                    name: m[1],
                    line: i,
                    range: new vscode.Range(i, 0, i, 0),
                    relPath,
                });
                break;
            }
        }
    }
    return results;
}

async function getHashAtCursor(): Promise<string | undefined> {
    const editor = vscode.window.activeTextEditor;
    if (!editor || !serverAvailable) return undefined;

    const pos = editor.selection.active;
    const wordRange = editor.document.getWordRangeAtPosition(pos);
    if (!wordRange) return undefined;

    const word = editor.document.getText(wordRange);
    const root = workspaceRoot();
    if (!root) return undefined;
    const relPath = path.relative(root, editor.document.uri.fsPath);

    try {
        const output = await httpGet(
            `/discover/${encodeURIComponent(word)}?file=${encodeURIComponent(relPath)}&line=${pos.line + 1}`,
        );
        const result: DiscoverResult = JSON.parse(output);
        return result.hash;
    } catch {
        return undefined;
    }
}
