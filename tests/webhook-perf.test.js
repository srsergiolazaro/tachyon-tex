const http = require('http');
// Use global FormData and Blob (Node 18+)

const BASE_URL = 'http://localhost:8080';
const WEBHOOK_PORT = 9999;
const WEBHOOK_URL = `http://host.docker.internal:${WEBHOOK_PORT}/webhook`;

// Store webhook timestamps
const webhookArrivals = new Map();

// ANSI colors
const GREEN = '\x1b[32m';
const RED = '\x1b[31m';
const YELLOW = '\x1b[33m';
const CYAN = '\x1b[36m';
const RESET = '\x1b[0m';
const BOLD = '\x1b[1m';

async function setupWebhookServer() {
    return new Promise((resolve) => {
        const server = http.createServer((req, res) => {
            if (req.url === '/webhook' && req.method === 'POST') {
                let body = '';
                req.on('data', chunk => body += chunk);
                req.on('end', () => {
                    const data = JSON.parse(body);
                    const arrivalTime = Date.now();

                    // We'll use the timestamp from the payload to correlate if needed, 
                    // or just use the order if we send sequentially.
                    // For this test, we'll store by arrival order.
                    webhookArrivals.set(webhookArrivals.size, {
                        time: arrivalTime,
                        data: data
                    });

                    res.writeHead(200);
                    res.end('OK');
                });
            } else {
                res.writeHead(404);
                res.end();
            }
        });

        server.listen(WEBHOOK_PORT, () => {
            console.log(`${CYAN}Listening for webhooks on port ${WEBHOOK_PORT}...${RESET}`);
            resolve(server);
        });
    });
}

async function registerWebhook() {
    const res = await fetch(`${BASE_URL}/webhooks`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
            url: WEBHOOK_URL,
            events: ['compile.success']
        })
    });

    if (!res.ok) {
        throw new Error(`Failed to register webhook: ${await res.text()}`);
    }
    const data = await res.json();
    console.log(`${GREEN}Registered webhook with ID: ${data.id}${RESET}`);
    return data.id;
}

async function compile(index) {
    const content = `\\documentclass{article}
\\begin{document}
Iteration ${index} - Random: ${Math.random()}
\\end{document}`;

    const formData = new FormData();
    formData.append('file', new Blob([content], { type: 'text/plain' }), 'test.tex');

    const start = Date.now();
    const res = await fetch(`${BASE_URL}/compile`, {
        method: 'POST',
        body: formData
    });
    const end = Date.now();

    if (!res.ok) {
        throw new Error(`Compilation failed: ${await res.text()}`);
    }

    return {
        rtt: end - start,
        engineTime: parseInt(res.headers.get('X-Compile-Time-Ms'))
    };
}

async function runBenchmark() {
    console.log(`\n${BOLD}🚀 Tachyon-Tex Webhook Performance Benchmark${RESET}\n`);

    let server;
    let webhookId;

    try {
        server = await setupWebhookServer();
        webhookId = await registerWebhook();

        const iterations = 5;
        const results = [];

        console.log(`\n${YELLOW}Starting ${iterations} compilation iterations...${RESET}\n`);

        for (let i = 0; i < iterations; i++) {
            const startIteration = Date.now();
            const { rtt, engineTime } = await compile(i);

            // Wait a bit for webhook to arrive (it should be nearly instant)
            let webhookReceived = false;
            let waitStart = Date.now();
            while (Date.now() - waitStart < 5000) {
                if (webhookArrivals.has(i)) {
                    webhookReceived = true;
                    break;
                }
                await new Promise(r => setTimeout(r, 10));
            }

            if (webhookReceived) {
                const webhookData = webhookArrivals.get(i);
                const webhookDelay = webhookData.time - startIteration;
                results.push({ iteration: i, rtt, engineTime, webhookDelay });
                console.log(`  Iter ${i}: RTT=${rtt}ms, Engine=${engineTime}ms, WebhookDelay=${webhookDelay}ms`);
            } else {
                console.log(`${RED}  Iter ${i}: Webhook never arrived!${RESET}`);
            }
        }

        console.log(`\n${BOLD}📊 Benchmark Summary${RESET}`);
        console.table(results);

        const avgRtt = results.reduce((acc, r) => acc + r.rtt, 0) / results.length;
        const avgWebhook = results.reduce((acc, r) => acc + r.webhookDelay, 0) / results.length;

        console.log(`\n${BOLD}Average API Round-Trip: ${CYAN}${avgRtt.toFixed(2)}ms${RESET}`);
        console.log(`${BOLD}Average Webhook Delivery: ${CYAN}${avgWebhook.toFixed(2)}ms${RESET}`);

        if (avgWebhook < avgRtt) {
            console.log(`\n${GREEN}✨ Webhook delivery is ${(avgRtt - avgWebhook).toFixed(2)}ms faster than waiting for response!${RESET}`);
        } else {
            console.log(`\n${YELLOW}ℹ️ Webhook delivery has slight overhead: ${(avgWebhook - avgRtt).toFixed(2)}ms${RESET}`);
        }

    } catch (err) {
        console.error(`${RED}Error:${RESET}`, err.message);
    } finally {
        if (webhookId) {
            await fetch(`${BASE_URL}/webhooks/${webhookId}`, { method: 'DELETE' }).catch(() => { });
        }
        if (server) server.close();
        process.exit();
    }
}

runBenchmark();
