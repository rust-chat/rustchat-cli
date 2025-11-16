#!/usr/bin/env node

const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const projectRoot = path.resolve(__dirname, '..');
const distDir = path.join(projectRoot, 'dist');
const binaryName = process.platform === 'win32' ? 'rustaichat.exe' : 'rustaichat';
const targetBinary = path.join(projectRoot, 'target', 'release', binaryName);
const distBinary = path.join(distDir, binaryName);

function ensureCargo() {
  const result = spawnSync('cargo', ['--version'], { stdio: 'ignore' });
  if (result.status !== 0) {
    console.error('[rustaichat] cargo is required to build the CLI. Install Rust from https://rustup.rs/.');
    process.exit(result.status || 1);
  }
}

function buildBinary() {
  console.log('[rustaichat] Building Rust binary (cargo build --release)...');
  const result = spawnSync('cargo', ['build', '--release'], {
    cwd: projectRoot,
    stdio: 'inherit',
  });
  if (result.status !== 0) {
    console.error('[rustaichat] cargo build failed. See output above.');
    process.exit(result.status || 1);
  }
}

function copyBinary() {
  if (!fs.existsSync(targetBinary)) {
    console.error(`[rustaichat] Expected binary not found at ${targetBinary}`);
    process.exit(1);
  }
  fs.mkdirSync(distDir, { recursive: true });
  fs.copyFileSync(targetBinary, distBinary);
  fs.chmodSync(distBinary, 0o755);
  console.log(`[rustaichat] CLI ready at ${distBinary}`);
}

ensureCargo();
buildBinary();
copyBinary();
