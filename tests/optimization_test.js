const { readdir } = require('node:fs/promises');

// Bun-native Optimization Test for Tachyon-Tex
const WS_URL = 'ws://localhost:8080/ws';

async function runTest() {
    console.log('🚀 Starting Moonshot Optimization Test (REAL PAPER)...');

    let phase2 = false;
    // Map to store original file buffers for hashing check later
    const projectFiles = new Map();
    const serverHashes = new Map();

    // 1. Load all files from test/ directory
    const testDir = 'test';
    const entries = await readdir(testDir, { withFileTypes: true });

    console.log(`📂 Loading files from ${testDir}...`);

    for (const entry of entries) {
        if (entry.isFile()) {
            const path = `${testDir}/${entry.name}`;
            if (entry.name.endsWith('.png') || entry.name.endsWith('.tex')) {
                const file = Bun.file(path);
                const arrayBuffer = await file.arrayBuffer();
                const buffer = Buffer.from(arrayBuffer);
                projectFiles.set(entry.name, buffer);
                console.log(`   - Loaded ${entry.name} (${(buffer.length / 1024).toFixed(1)} KB)`);
            }
        }
    }

    const socket = new WebSocket(WS_URL);

    socket.addEventListener("open", async () => {
        console.log('✅ Connected to Tachyon-Tex Engine');

        // Prepare Phase 1: Initial upload
        const payload1 = {
            main: "main.tex",
            files: {}
        };

        for (const [name, buffer] of projectFiles) {
            if (name.endsWith('.tex')) {
                payload1.files[name] = buffer.toString('utf-8');
            } else {
                payload1.files[name] = buffer.toString('base64');
            }
        }

        console.log(`📦 Phase 1: Sending project (${projectFiles.size} files, ~${(JSON.stringify(payload1).length / 1024 / 1024).toFixed(2)} MB)...`);
        socket.send(JSON.stringify(payload1));
    });

    socket.addEventListener("message", async (event) => {
        const msg = JSON.parse(event.data);

        if (msg.type === 'compile_success') {
            if (!phase2) {
                console.log(`✨ Phase 1 Success: PDF generated in ${msg.compile_time_ms}ms`);

                // Save server hashes
                const blobs = msg.blobs || {};
                let mappedCount = 0;
                for (const [name, hash] of Object.entries(blobs)) {
                    serverHashes.set(name, hash);
                    mappedCount++;
                }
                console.log(`🔑 Server returned ${mappedCount} blob hashes.`);

                phase2 = true;

                // Modify main.tex for Phase 2 (Append comment)
                let mainContent = projectFiles.get('main.tex').toString('utf-8');
                mainContent += `\n% Live Update Check ${Date.now()}\n`;

                const payload2 = {
                    main: "main.tex",
                    files: {}
                };

                // Add main.tex (changed)
                payload2.files["main.tex"] = mainContent;

                // Add Images (Using Hashes if available)
                let reusedCount = 0;
                for (const [name, buffer] of projectFiles) {
                    if (name.endsWith('.png')) {
                        if (serverHashes.has(name)) {
                            payload2.files[name] = { type: 'hash', value: serverHashes.get(name) };
                            reusedCount++;
                        } else {
                            // First time or text file
                            payload2.files[name] = buffer.toString('base64');
                        }
                    }
                }

                console.log(`\n⚡ Phase 2: Sending Delta Sync (Reused ${reusedCount} images)...`);
                await new Promise(r => setTimeout(r, 500));
                socket.send(JSON.stringify(payload2));
            } else {
                console.log(`🚀 Phase 2 Success: PDF updated in ${msg.compile_time_ms}ms`);
                console.log(`📈 SUCCESS: Real Paper Optimized.`);
                socket.close();
                process.exit(0);
            }
        } else if (msg.type === 'compile_error') {
            console.error('❌ Compile Error:', msg.error);
            socket.close();
            process.exit(1);
        }
    });

    socket.addEventListener("error", (e) => {
        console.error('❌ WebSocket Error:', e.message);
    });

    socket.addEventListener("close", () => {
        if (!phase2) {
            console.error('❌ Connection closed before Phase 2');
            process.exit(1);
        }
    });
}

runTest();
