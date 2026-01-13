/**
 * Tachyon-Tex API Test Suite
 * 
 * Tests for all endpoints: /compile, /validate, /packages
 * 
 * Usage:
 *   1. Start the Docker container: docker run -p 8080:8080 tachyon-tex
 *   2. Run tests: node tests/api.test.js
 * 
 * Requirements:
 *   - Node.js 18+ (for native fetch)
 */

const BASE_URL = process.env.TACHYON_URL || 'http://localhost:8080';

// ANSI colors
const GREEN = '\x1b[32m';
const RED = '\x1b[31m';
const YELLOW = '\x1b[33m';
const CYAN = '\x1b[36m';
const RESET = '\x1b[0m';
const BOLD = '\x1b[1m';

let passed = 0;
let failed = 0;

function log(status, testName, details = '') {
    const icon = status === 'pass' ? `${GREEN}✓${RESET}` : status === 'fail' ? `${RED}✗${RESET}` : `${YELLOW}⏳${RESET}`;
    console.log(`  ${icon} ${testName}${details ? ` ${CYAN}${details}${RESET}` : ''}`);
}

async function test(name, fn) {
    try {
        await fn();
        passed++;
        log('pass', name);
    } catch (error) {
        failed++;
        log('fail', name, `- ${error.message}`);
    }
}

function assert(condition, message) {
    if (!condition) throw new Error(message);
}

// ============================================================================
// Test Data
// ============================================================================

const SIMPLE_TEX = `\\documentclass{article}
\\begin{document}
Hello, Tachyon-Tex!
\\end{document}`;

const INVALID_TEX_MISSING_END = `\\documentclass{article}
\\begin{document}
Hello, World!`;

const INVALID_TEX_UNBALANCED_BRACES = `\\documentclass{article}
\\begin{document}
\\textbf{Hello
\\end{document}`;

const INVALID_TEX_ENV_MISMATCH = `\\documentclass{article}
\\begin{document}
\\begin{itemize}
\\item Test
\\end{enumerate}
\\end{document}`;

const TEX_WITH_WARNINGS = `\\documentclass{article}
\\begin{document}
$$E = mc^2$$
This is \\bf deprecated.
\\end{document}`;

const MATH_TEX = `\\documentclass{article}
\\usepackage{amsmath}
\\begin{document}
\\begin{equation}
E = mc^2
\\end{equation}
\\end{document}`;

// ============================================================================
// Tests
// ============================================================================

async function runTests() {
    console.log(`\n${BOLD}🧪 Tachyon-Tex API Tests${RESET}`);
    console.log(`   Target: ${CYAN}${BASE_URL}${RESET}\n`);

    // -------------------------------------------------------------------------
    // Health Check
    // -------------------------------------------------------------------------
    console.log(`${BOLD}📡 Health Check${RESET}`);

    await test('GET / returns 200', async () => {
        const res = await fetch(`${BASE_URL}/`);
        assert(res.status === 200, `Expected 200, got ${res.status}`);
    });

    await test('GET / returns HTML', async () => {
        const res = await fetch(`${BASE_URL}/`);
        const contentType = res.headers.get('content-type');
        assert(contentType.includes('text/html'), `Expected text/html, got ${contentType}`);
    });

    // -------------------------------------------------------------------------
    // GET /packages
    // -------------------------------------------------------------------------
    console.log(`\n${BOLD}📦 GET /packages${RESET}`);

    await test('Returns JSON with package list', async () => {
        const res = await fetch(`${BASE_URL}/packages`);
        assert(res.status === 200, `Expected 200, got ${res.status}`);
        const data = await res.json();
        assert(data.packages, 'Missing packages array');
        assert(data.count > 0, 'Expected at least one package');
    });

    await test('Contains common packages (amsmath, graphicx, tikz)', async () => {
        const res = await fetch(`${BASE_URL}/packages`);
        const data = await res.json();
        const names = data.packages.map(p => p.name);
        assert(names.includes('amsmath'), 'Missing amsmath');
        assert(names.includes('graphicx'), 'Missing graphicx');
        assert(names.includes('tikz'), 'Missing tikz');
    });

    await test('Packages have name, description, category', async () => {
        const res = await fetch(`${BASE_URL}/packages`);
        const data = await res.json();
        const pkg = data.packages[0];
        assert(pkg.name, 'Missing name');
        assert(pkg.description, 'Missing description');
        assert(pkg.category, 'Missing category');
    });

    // -------------------------------------------------------------------------
    // POST /validate
    // -------------------------------------------------------------------------
    console.log(`\n${BOLD}✅ POST /validate${RESET}`);

    await test('Valid LaTeX returns valid: true', async () => {
        const formData = new FormData();
        formData.append('file', new Blob([SIMPLE_TEX], { type: 'text/plain' }), 'test.tex');

        const res = await fetch(`${BASE_URL}/validate`, { method: 'POST', body: formData });
        assert(res.status === 200, `Expected 200, got ${res.status}`);
        const data = await res.json();
        assert(data.valid === true, `Expected valid: true, got ${data.valid}`);
        assert(data.errors.length === 0, `Expected no errors, got ${data.errors.length}`);
    });

    await test('Missing \\end{document} returns error', async () => {
        const formData = new FormData();
        formData.append('file', new Blob([INVALID_TEX_MISSING_END], { type: 'text/plain' }), 'test.tex');

        const res = await fetch(`${BASE_URL}/validate`, { method: 'POST', body: formData });
        const data = await res.json();
        assert(data.valid === false, 'Expected valid: false');
        assert(data.errors.some(e => e.message.includes('end{document}')), 'Expected end{document} error');
    });

    await test('Environment mismatch detected', async () => {
        const formData = new FormData();
        formData.append('file', new Blob([INVALID_TEX_ENV_MISMATCH], { type: 'text/plain' }), 'test.tex');

        const res = await fetch(`${BASE_URL}/validate`, { method: 'POST', body: formData });
        const data = await res.json();
        assert(data.valid === false, 'Expected valid: false');
        assert(data.errors.some(e => e.message.includes('mismatch')), 'Expected mismatch error');
    });

    await test('Deprecated commands generate warnings', async () => {
        const formData = new FormData();
        formData.append('file', new Blob([TEX_WITH_WARNINGS], { type: 'text/plain' }), 'test.tex');

        const res = await fetch(`${BASE_URL}/validate`, { method: 'POST', body: formData });
        const data = await res.json();
        assert(data.warnings.length > 0, 'Expected warnings');
        assert(data.warnings.some(w => w.includes('$$') || w.includes('bf')), 'Expected $$ or \\bf warning');
    });

    await test('No file returns error', async () => {
        const formData = new FormData();
        const res = await fetch(`${BASE_URL}/validate`, { method: 'POST', body: formData });
        const data = await res.json();
        assert(data.valid === false, 'Expected valid: false');
    });

    // -------------------------------------------------------------------------
    // POST /compile (Multi-file without ZIP)
    // -------------------------------------------------------------------------
    console.log(`\n${BOLD}📄 POST /compile (Multi-file)${RESET}`);

    await test('Single .tex file compiles to PDF', async () => {
        const formData = new FormData();
        formData.append('file', new Blob([SIMPLE_TEX], { type: 'text/plain' }), 'doc.tex');

        const res = await fetch(`${BASE_URL}/compile`, { method: 'POST', body: formData });
        assert(res.status === 200, `Expected 200, got ${res.status}`);
        assert(res.headers.get('content-type') === 'application/pdf', 'Expected PDF');
        assert(res.headers.get('x-compile-time-ms'), 'Missing X-Compile-Time-Ms header');
    });

    await test('Multiple files compile correctly', async () => {
        const mainTex = `\\documentclass{article}
\\input{content}
\\begin{document}
\\mycontent
\\end{document}`;
        const contentTex = `\\newcommand{\\mycontent}{Hello from included file!}`;

        const formData = new FormData();
        formData.append('main', new Blob([mainTex], { type: 'text/plain' }), 'main.tex');
        formData.append('content', new Blob([contentTex], { type: 'text/plain' }), 'content.tex');

        const res = await fetch(`${BASE_URL}/compile`, { method: 'POST', body: formData });
        assert(res.status === 200, `Expected 200, got ${res.status}`);
        assert(res.headers.get('x-files-received') === '2', 'Expected 2 files received');
    });

    await test('Auto-detects main file with \\begin{document}', async () => {
        const helperTex = `\\newcommand{\\helper}{Helper}`;
        const realMain = `\\documentclass{article}
\\input{helper}
\\begin{document}
\\helper
\\end{document}`;

        const formData = new FormData();
        // Send helper first to test detection
        formData.append('f1', new Blob([helperTex], { type: 'text/plain' }), 'helper.tex');
        formData.append('f2', new Blob([realMain], { type: 'text/plain' }), 'paper.tex');

        const res = await fetch(`${BASE_URL}/compile`, { method: 'POST', body: formData });
        assert(res.status === 200, `Expected 200, got ${res.status}`);
    });

    await test('No files returns 400', async () => {
        const formData = new FormData();
        const res = await fetch(`${BASE_URL}/compile`, { method: 'POST', body: formData });
        assert(res.status === 400, `Expected 400, got ${res.status}`);
    });

    // -------------------------------------------------------------------------
    // Performance
    // -------------------------------------------------------------------------
    console.log(`\n${BOLD}⚡ Performance${RESET}`);

    await test('Compile time < 10s for simple document', async () => {
        const formData = new FormData();
        formData.append('file', new Blob([SIMPLE_TEX], { type: 'text/plain' }), 'test.tex');

        const start = Date.now();
        const res = await fetch(`${BASE_URL}/compile`, { method: 'POST', body: formData });
        const elapsed = Date.now() - start;

        assert(res.status === 200, `Compilation failed with ${res.status}`);
        assert(elapsed < 10000, `Took ${elapsed}ms, expected < 10000ms`);
        log('pass', `  Actual time: ${elapsed}ms`);
    });

    await test('Math document compiles successfully', async () => {
        const formData = new FormData();
        formData.append('file', new Blob([MATH_TEX], { type: 'text/plain' }), 'math.tex');

        const res = await fetch(`${BASE_URL}/compile`, { method: 'POST', body: formData });
        assert(res.status === 200, `Expected 200, got ${res.status}`);
    });

    // -------------------------------------------------------------------------
    // Summary
    // -------------------------------------------------------------------------
    console.log(`\n${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}`);
    console.log(`${BOLD}Results:${RESET} ${GREEN}${passed} passed${RESET}, ${failed > 0 ? RED : ''}${failed} failed${RESET}`);
    console.log(`${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}\n`);

    process.exit(failed > 0 ? 1 : 0);
}

runTests().catch(err => {
    console.error(`${RED}Test runner error:${RESET}`, err);
    process.exit(1);
});
