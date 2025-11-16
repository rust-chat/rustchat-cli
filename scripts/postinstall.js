const https = require('https');
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const OWNER = 'rust-chat';
const REPO = 'rustchat-cli';
const PACKAGE = 'rustchat-cli';

function mapAssetName() {
    const plat = process.platform; // 'win32', 'linux', 'darwin'

    if (plat === 'win32') return 'rustchat-cli-windows-x86_64.exe';
    if (plat === 'linux') return 'rustchat-cli-linux-x86_64';
    if (plat === 'darwin') return 'rustchat-cli-macos-x86_64';

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
    console.log(`[${PACKAGE}] Installing prebuilt binary for current OS...`);

    const asset = mapAssetName();
    if (!asset) {
        console.warn(`[${PACKAGE}] Unsupported platform. Skipping install.`);
        return;
    }

    // OS별 폴더로 저장
    const distDir = path.join(__dirname, '..', 'dist', process.platform);
    if (!fs.existsSync(distDir)) fs.mkdirSync(distDir, { recursive: true });

    const targetName = asset.endsWith('.exe') ? 'rustchat-cli.exe' : 'rustchat-cli';
    const dest = path.join(distDir, targetName);

    try {
        await downloadAsset(asset, dest);
        if (process.platform !== 'win32') fs.chmodSync(dest, 0o755);
        console.log(`[${PACKAGE}] Installed ${targetName} -> ${dest}`);
    } catch (err) {
        console.warn(`[${PACKAGE}] Download failed:`, err.message);
        console.log(`[${PACKAGE}] Attempting local build...`);

        try {
            execSync('cargo build --release', { stdio: 'inherit' });
            const builtName = process.platform === 'win32' ? 'rustchat-cli.exe' : 'rustchat-cli';
            const builtPath = path.join(__dirname, '..', 'target', 'release', builtName);

            if (fs.existsSync(builtPath)) {
                fs.copyFileSync(builtPath, dest);
                if (process.platform !== 'win32') fs.chmodSync(dest, 0o755);
                console.log(`[${PACKAGE}] Local build copied to ${dest}`);
            } else {
                console.warn(`[${PACKAGE}] Local build completed but binary not found.`);
            }
        } catch {
            console.warn(`[${PACKAGE}] Local build failed. Skipping.`);
        }
    }
}

main().catch(err => console.error(`[${PACKAGE}] postinstall error:`, err));
