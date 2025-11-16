#!/usr/bin/env node

const { spawn } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const projectRoot = path.resolve(__dirname, '..');
const binaryName = process.platform === 'win32' ? 'rustaichat.exe' : 'rustaichat';
const binaryPath = path.join(projectRoot, 'dist', binaryName);

if (!fs.existsSync(binaryPath)) {
  console.error('[rustaichat] Compiled binary not found. Run "npm install" to build it.');
  process.exit(1);
}

const child = spawn(binaryPath, process.argv.slice(2), {
  stdio: 'inherit',
  env: process.env,
});

child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
  } else {
    process.exit(code ?? 0);
  }
});
