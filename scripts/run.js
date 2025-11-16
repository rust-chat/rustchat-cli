#!/usr/bin/env node
const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

const platformMap = {
  win32: 'win32',
  linux: 'linux',
  darwin: 'darwin',
};

const platDir = platformMap[process.platform];
if (!platDir) {
    console.error('[rustaichat] Unsupported platform');
    process.exit(1);
}

const distDir = path.join(__dirname, '..', 'dist', platDir);
const binaryName = process.platform === 'win32' ? 'rustaichat.exe' : 'rustaichat';
const binaryPath = path.join(distDir, binaryName);

if (!fs.existsSync(binaryPath)) {
    console.error(`[rustaichat] Binary not found at ${binaryPath}`);
    process.exit(1);
}

const child = spawn(binaryPath, process.argv.slice(2), { stdio: 'inherit', env: process.env });
child.on('exit', (code, signal) => {
    if (signal) process.kill(process.pid, signal);
    else process.exit(code ?? 0);
});
