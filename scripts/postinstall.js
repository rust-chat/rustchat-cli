const https = require('https');
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const OWNER = 'wnsdud-jy';
const REPO = 'rustaichat';

function mapAssetName() {
    const plat = process.platform; // 'win32', 'linux', 'darwin'
    const arch = process.arch;     // 'x64', 'arm64'

    if (plat === 'win32') return 'rustaichat-windows-x86_64.exe';
    if (plat === 'linux') return 'rustaichat-linux-x86_64';
    if (plat === 'darwin') return 'rustaichat-macos-x86_64';

    return null;
}

function downloadAsset(assetName, destPath) {
    return new Promise((resolve, reject) => {
        const url = `https://github.com/${OWNER}/${REPO}/releases/latest/download/${assetName}`;
        const file = fs.createWriteStream(destPath, { mode: 0o755 });

        https.get(url, (res) => {
            if (res.statusCode >= 400) return reject(new Error(`Failed ${url} - status ${res.statusCode}`));
            res.pipe(file);
            file.on('finish', () => file.close(resolve));
        }).on('error', (err) => {
            fs.unlink(destPath, () => {});
            reject(err);
        });
    });
}

async function main() {
    console.log('[rustaichat] Installing prebuilt binary if available...');
    const asset = mapAssetName();
    const distDir = path.join(__dirname, '..', 'dist');
    if (!fs.existsSync(distDir)) fs.mkdirSync(distDir, { recursive: true });

    if (!asset) {
        console.warn('[rustaichat] Unsupported platform/arch. Skipping automatic install.');
        return;
    }

    const targetName = asset.endsWith('.exe') ? 'rustaichat.exe' : 'rustaichat';
    const dest = path.join(distDir, targetName);

    try {
        await downloadAsset(asset, dest);
        if (process.platform !== 'win32') fs.chmodSync(dest, 0o755);
        console.log(`[rustaichat] Installed ${targetName} -> ${dest}`);
    } catch (err) {
        console.warn('[rustaichat] Could not download prebuilt binary:', err.message);
        console.log('[rustaichat] Attempting local build with `cargo build --release`...');

        try {
            execSync('cargo build --release', { stdio: 'inherit' });
            const builtName = process.platform === 'win32' ? 'rustaichat.exe' : 'rustaichat';
            const builtPath = path.join(__dirname, '..', 'target', 'release', builtName);

            if (fs.existsSync(builtPath)) {
                fs.copyFileSync(builtPath, dest);
                if (process.platform !== 'win32') fs.chmodSync(dest, 0o755);
                console.log(`[rustaichat] Local build copied to ${dest}`);
            } else {
                console.warn('[rustaichat] Local build completed but binary not found.');
            }
        } catch (buildErr) {
            console.warn('[rustaichat] Local build failed or cargo not available. Skipping.');
        }
    }
}

main().catch(err => console.error('[rustaichat] postinstall error:', err));
