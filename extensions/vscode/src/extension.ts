import * as vscode from 'vscode';

export function activate(context: vscode.ExtensionContext) {
    const compileCmd = vscode.commands.registerCommand('keel.compile', () => {
        vscode.window.showInformationMessage('Keel: compile not yet implemented');
    });

    const discoverCmd = vscode.commands.registerCommand('keel.discover', () => {
        vscode.window.showInformationMessage('Keel: discover not yet implemented');
    });

    const statsCmd = vscode.commands.registerCommand('keel.stats', () => {
        vscode.window.showInformationMessage('Keel: stats not yet implemented');
    });

    context.subscriptions.push(compileCmd, discoverCmd, statsCmd);
}

export function deactivate() {}
