#!/usr/bin/env node
const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

const PACKAGE_NAME = 'rustchat-cli';

const platformMap = {
  win32: 'win32',
  linux: 'linux',
  darwin: 'darwin',
};

const platDir = platformMap[process.platform];
if (!platDir) {
  console.error(`[${PACKAGE_NAME}] Unsupported platform`);
  process.exit(1);
}

const distDir = path.join(__dirname, '..', 'dist', platDir);

// Windows에서 실제 존재하는 exe 파일 찾기
let binaryName;
if (process.platform === 'win32') {
  const candidates = ['rustchat-cli-windows-x86_64.exe'];
  binaryName = candidates.find(f => fs.existsSync(path.join(distDir, f)));
} else if (process.platform === 'linux') {
  const candidates = ['rustchat-cli-linux-x86_64'];
  binaryName = candidates.find(f => fs.existsSync(path.join(distDir, f)));
} else if (process.platform === 'darwin') {
  const candidates = ['rustchat-cli-macos-x86_64'];
  binaryName = candidates.find(f => fs.existsSync(path.join(distDir, f)));
}

if (!binaryName) {
  console.error(`[${PACKAGE_NAME}] Binary not found in ${distDir}`);
  process.exit(1);
}

const binaryPath = path.join(distDir, binaryName);

const child = spawn(binaryPath, process.argv.slice(2), { stdio: 'inherit', env: process.env });

child.on('exit', (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  else process.exit(code ?? 0);
});
