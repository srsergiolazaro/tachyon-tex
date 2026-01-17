// HMR v2 Benchmark Test
// Tests the Format Cache system for preamble-based compilation speedup

const BASE_URL = 'http://localhost:8082';

const LATEX_DOC_V1 = `\\documentclass{article}
\\usepackage{amsmath}
\\usepackage{graphicx}
\\begin{document}
Hello, World! Version 1.
\\end{document}`;

const LATEX_DOC_V2 = `\\documentclass{article}
\\usepackage{amsmath}
\\usepackage{graphicx}
\\begin{document}
Hello, World! Version 2 - only body changed.
\\end{document}`;

const LATEX_DOC_V3_NEW_PREAMBLE = `\\documentclass{article}
\\usepackage{amsmath}
\\usepackage{graphicx}
\\usepackage{hyperref}
\\begin{document}
Hello, World! Version 3 - new package in preamble.
\\end{document}`;

async function compile(latex, label) {
    const formData = new FormData();
    formData.append('file', new Blob([latex], { type: 'text/plain' }), 'main.tex');
    
    const start = performance.now();
    const response = await fetch(`${BASE_URL}/compile`, {
        method: 'POST',
        body: formData
    });
    const elapsed = performance.now() - start;
    
    const compileTime = response.headers.get('X-Compile-Time-Ms');
    const hmrStatus = response.headers.get('X-HMR');
    const preambleHash = response.headers.get('X-Preamble-Hash');
    const cache = response.headers.get('X-Cache');
    
    console.log(`[${label}] Status: ${response.status}, Compile: ${compileTime}ms, HMR: ${hmrStatus}, Hash: ${preambleHash}, RTT: ${elapsed.toFixed(0)}ms`);
    
    return { compileTime: parseInt(compileTime), hmrStatus, preambleHash, elapsed };
}

async function runBenchmark() {
    console.log('='.repeat(60));
    console.log('HMR v2 Benchmark - Format Cache Test');
    console.log('='.repeat(60));
    
    // Wait for server to be ready
    await new Promise(r => setTimeout(r, 2000));
    
    // Test 1: First compilation (cold start)
    console.log('\n--- Test 1: First compilation (format not cached) ---');
    const result1 = await compile(LATEX_DOC_V1, 'V1-Cold');
    
    // Test 2: Same preamble, different body (should be HIT)
    console.log('\n--- Test 2: Same preamble, different body (should be HIT) ---');
    const result2 = await compile(LATEX_DOC_V2, 'V2-Warm');
    
    // Test 3: Another with same preamble
    console.log('\n--- Test 3: Same preamble again ---');
    const result3 = await compile(LATEX_DOC_V1, 'V1-Warm');
    
    // Test 4: New preamble (new package added)
    console.log('\n--- Test 4: New preamble (hyperref added) ---');
    const result4 = await compile(LATEX_DOC_V3_NEW_PREAMBLE, 'V3-Cold');
    
    // Test 5: Back to v3 preamble (should be HIT now)
    console.log('\n--- Test 5: V3 preamble again (should be HIT) ---');
    const result5 = await compile(LATEX_DOC_V3_NEW_PREAMBLE, 'V3-Warm');
    
    // Summary
    console.log('\n' + '='.repeat(60));
    console.log('Summary:');
    console.log('='.repeat(60));
    console.log(`V1 Cold: ${result1.compileTime}ms (HMR: ${result1.hmrStatus})`);
    console.log(`V2 Warm: ${result2.compileTime}ms (HMR: ${result2.hmrStatus})`);
    console.log(`V1 Warm: ${result3.compileTime}ms (HMR: ${result3.hmrStatus})`);
    console.log(`V3 Cold: ${result4.compileTime}ms (HMR: ${result4.hmrStatus})`);
    console.log(`V3 Warm: ${result5.compileTime}ms (HMR: ${result5.hmrStatus})`);
    
    const speedup = result1.compileTime > 0 ? (result1.compileTime / result2.compileTime).toFixed(2) : 'N/A';
    console.log(`\nSpeedup (Cold vs Warm): ${speedup}x`);
    
    // Verify preamble hashes
    console.log('\nPreamble Hash Verification:');
    console.log(`V1/V2 same hash: ${result1.preambleHash === result2.preambleHash ? '✅ YES' : '❌ NO'}`);
    console.log(`V3 different hash: ${result1.preambleHash !== result4.preambleHash ? '✅ YES' : '❌ NO'}`);
}

runBenchmark().catch(console.error);
