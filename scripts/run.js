#!/usr/bin/env node

const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

const distDir = path.join(__dirname, '..', 'dist');
const binaryName = process.platform === 'win32' ? 'rustaichat.exe' : 'rustaichat';
const binaryPath = path.join(distDir, binaryName);

if (!fs.existsSync(binaryPath)) {
    console.error('[rustaichat] Binary not found. Run `npm install` to build/download it.');
    process.exit(1);
}

const child = spawn(binaryPath, process.argv.slice(2), {
    stdio: 'inherit',
    env: process.env,
});

child.on('exit', (code, signal) => {
    if (signal) process.kill(process.pid, signal);
    else process.exit(code ?? 0);
});
