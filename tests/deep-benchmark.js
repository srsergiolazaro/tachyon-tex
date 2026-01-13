/**
 * Deep Benchmark: Analiza exactamente dónde se va el tiempo de compilación.
 */

const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const BASE_URL = 'http://localhost:8080';

// IEEE Template mínimo (sin imágenes)
const IEEE_MINIMAL = `\\documentclass[conference]{IEEEtran}
\\IEEEoverridecommandlockouts
\\usepackage{cite}
\\usepackage{amsmath,amssymb,amsfonts}
\\usepackage{algorithmic}
\\usepackage{graphicx}
\\usepackage{textcomp}
\\usepackage{xcolor}
\\usepackage{hyperref}

\\begin{document}
\\title{Test Document}
\\author{\\IEEEauthorblockN{Author Name}
\\IEEEauthorblockA{Institution}}
\\maketitle
\\begin{abstract}
This is a minimal test document.
\\end{abstract}
\\section{Introduction}
Hello world.
\\end{document}`;

// IEEE con texto largo (aproximadamente igual al tuyo)
const IEEE_LONG_TEXT = `\\documentclass[conference]{IEEEtran}
\\IEEEoverridecommandlockouts
\\usepackage{cite}
\\usepackage{amsmath,amssymb,amsfonts}
\\usepackage{algorithmic}
\\usepackage{graphicx}
\\usepackage{textcomp}
\\usepackage{xcolor}
\\usepackage{hyperref}

\\begin{document}
\\title{Pure JavaScript Implementation for Scale-Invariant Feature Extraction}
\\author{\\IEEEauthorblockN{Author Name}
\\IEEEauthorblockA{Institution}}
\\maketitle
\\begin{abstract}
${'Lorem ipsum dolor sit amet. '.repeat(50)}
\\end{abstract}
\\section{Introduction}
${'Paragraph of text for testing purposes. '.repeat(100)}
\\section{Methodology}
${'More text content here. '.repeat(100)}
\\begin{equation}
E = mc^2
\\end{equation}
\\section{Results}
${'Additional content. '.repeat(100)}
\\begin{table}[htbp]
\\caption{Test Table}
\\begin{center}
\\begin{tabular}{|l|c|c|}
\\hline
\\textbf{Metric} & \\textbf{Before} & \\textbf{After} \\\\
\\hline
Time & 2.31 & 0.94 \\\\
\\hline
\\end{tabular}
\\end{center}
\\end{table}
\\section{Conclusion}
${'Final text. '.repeat(50)}
\\end{document}`;

// Hello World absoluto mínimo
const HELLO_WORLD = `\\documentclass{article}
\\begin{document}
Hello World!
\\end{document}`;

async function benchmark(name, content) {
    const formData = new FormData();
    formData.append('file', new Blob([content], { type: 'text/plain' }), 'main.tex');

    const times = [];
    const engineTimes = [];

    // 3 iteraciones para promediar
    for (let i = 0; i < 3; i++) {
        const start = Date.now();
        const resp = await fetch(`${BASE_URL}/compile`, { method: 'POST', body: formData });
        const rtt = Date.now() - start;

        if (resp.ok) {
            const engine = parseInt(resp.headers.get('x-compile-time-ms') || '0');
            times.push(rtt);
            engineTimes.push(engine);
        }
    }

    const avgRTT = times.reduce((a, b) => a + b, 0) / times.length;
    const avgEngine = engineTimes.reduce((a, b) => a + b, 0) / engineTimes.length;

    console.log(`  ${name.padEnd(35)} RTT: ${avgRTT.toFixed(0)}ms  Engine: ${avgEngine.toFixed(0)}ms`);
    return { name, avgRTT, avgEngine };
}

async function benchmarkWithImages() {
    const TEST_DIR = path.resolve(__dirname, '../test');
    const ZIP_PATH = path.resolve(__dirname, '../benchmark_test.zip');

    // Crear ZIP
    execSync(`pwsh -Command "Compress-Archive -Path '${TEST_DIR}\\*' -DestinationPath '${ZIP_PATH}' -Force"`);

    const fileBuffer = fs.readFileSync(ZIP_PATH);
    const times = [];
    const engineTimes = [];

    for (let i = 0; i < 3; i++) {
        const formData = new FormData();
        formData.append('file', new Blob([fileBuffer], { type: 'application/zip' }), 'project.zip');

        const start = Date.now();
        const resp = await fetch(`${BASE_URL}/compile`, { method: 'POST', body: formData });
        const rtt = Date.now() - start;

        if (resp.ok) {
            const engine = parseInt(resp.headers.get('x-compile-time-ms') || '0');
            times.push(rtt);
            engineTimes.push(engine);
        }
    }

    fs.unlinkSync(ZIP_PATH);

    const avgRTT = times.reduce((a, b) => a + b, 0) / times.length;
    const avgEngine = engineTimes.reduce((a, b) => a + b, 0) / engineTimes.length;

    console.log(`  ${'Full IEEE + 10 Images (~5MB)'.padEnd(35)} RTT: ${avgRTT.toFixed(0)}ms  Engine: ${avgEngine.toFixed(0)}ms`);
    return { name: 'Full IEEE + Images', avgRTT, avgEngine };
}

async function run() {
    console.log('\n🔬 DEEP BENCHMARK: Análisis de tiempos de compilación');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    const results = [];

    console.log('📝 Escenario 1: Documentos sin imágenes');
    results.push(await benchmark('Hello World (article)', HELLO_WORLD));
    results.push(await benchmark('IEEE Minimal (con paquetes)', IEEE_MINIMAL));
    results.push(await benchmark('IEEE Long Text (~300 líneas)', IEEE_LONG_TEXT));

    console.log('\n📷 Escenario 2: Documento completo con imágenes');
    results.push(await benchmarkWithImages());

    console.log('\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('📊 ANÁLISIS:');

    const helloWorld = results.find(r => r.name.includes('Hello'));
    const ieeeMinimal = results.find(r => r.name.includes('Minimal'));
    const ieeeText = results.find(r => r.name.includes('Long'));
    const full = results.find(r => r.name.includes('Images'));

    console.log(`\n  ⏱️  Overhead de paquetes IEEE: ${(ieeeMinimal.avgEngine - helloWorld.avgEngine).toFixed(0)}ms`);
    console.log(`  ⏱️  Overhead de texto largo:   ${(ieeeText.avgEngine - ieeeMinimal.avgEngine).toFixed(0)}ms`);
    console.log(`  ⏱️  Overhead de imágenes:      ${(full.avgEngine - ieeeText.avgEngine).toFixed(0)}ms`);
    console.log(`\n  🎯 Tiempo base (motor):        ${helloWorld.avgEngine.toFixed(0)}ms`);
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');
}

run().catch(console.error);
