const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const BASE_URL = 'http://localhost:8080';
const TEST_DIR = path.resolve(__dirname, '../test');
const ZIP_PATH = path.resolve(__dirname, '../test_project.zip');

async function createZip(source, out) {
    const cmd = `pwsh -Command "Compress-Archive -Path '${source}\\*' -DestinationPath '${out}' -Force"`;
    execSync(cmd);
}

async function runBenchmark() {
    console.log('🚀 Starting Benchmark...');
    console.log(`📂 Folder: ${TEST_DIR}`);

    if (!fs.existsSync(ZIP_PATH)) {
        console.log('📦 Creating ZIP from test folder...');
        await createZip(TEST_DIR, ZIP_PATH);
    } else {
        console.log('📦 ZIP already exists, reusing...');
    }

    const fileBuffer = fs.readFileSync(ZIP_PATH);
    const results = [];
    const iterations = 5;

    console.log(`\n⚡ Running ${iterations} iterations against ${BASE_URL}/compile...\n`);

    for (let i = 1; i <= iterations; i++) {
        const formData = new FormData();
        const blob = new Blob([fileBuffer], { type: 'application/zip' });
        formData.append('file', blob, 'project.zip');

        const start = Date.now();
        try {
            const resp = await fetch(`${BASE_URL}/compile`, {
                method: 'POST',
                body: formData,
            });

            const rtt = Date.now() - start;

            if (resp.ok) {
                const engineTime = resp.headers.get('x-compile-time-ms');
                const pdfSize = (await resp.arrayBuffer()).byteLength;
                results.push({ iteration: i, rtt, engineTime: parseInt(engineTime), size: pdfSize });
                console.log(`  [${i}] ✅ Success: RTT=${rtt}ms, Engine=${engineTime}ms, Size=${(pdfSize / 1024).toFixed(1)}KB`);
            } else {
                const text = await resp.text();
                console.error(`  [${i}] ❌ Failed: ${resp.status} ${text}`);
            }
        } catch (err) {
            console.error(`  [${i}] ❌ Error: ${err.message}`);
        }
    }

    if (results.length > 0) {
        const avgRTT = results.reduce((a, b) => a + b.rtt, 0) / results.length;
        const avgEngine = results.reduce((a, b) => a + b.engineTime, 0) / results.length;

        console.log('\n📊 Benchmark Summary');
        console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
        console.log(`Average RTT:        ${avgRTT.toFixed(2)}ms`);
        console.log(`Average Engine:     ${avgEngine.toFixed(2)}ms`);
        console.log(`Overhead (Network): ${(avgRTT - avgEngine).toFixed(2)}ms`);
        console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');
    }

    // Cleanup zip
    if (fs.existsSync(ZIP_PATH)) {
        fs.unlinkSync(ZIP_PATH);
    }
}

runBenchmark().catch(console.error);
