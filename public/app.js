// DOM Elements
const editor = document.getElementById('editor');
const statusDot = document.getElementById('status-dot');
const statusText = document.getElementById('status-text');
const compileTimeEl = document.getElementById('compile-time');
const emptyState = document.getElementById('empty-state');
const errorToast = document.getElementById('error-toast');
const frame1 = document.getElementById('pdf-1');
const frame2 = document.getElementById('pdf-2');
const fileList = document.getElementById('file-list');
const activeFileName = document.getElementById('active-file-name');
const assetUpload = document.getElementById('asset-upload');

// Panels for Mobile
const editorPanel = document.getElementById('editor-panel');
const previewPanel = document.getElementById('preview-panel');
const fileTree = document.getElementById('file-tree');

// Project State (v1.3)
let project = {
    main: "main.tex",
    files: {
        "main.tex": "\\documentclass{article}\n\\usepackage[utf8]{inputenc}\n\\usepackage{amsmath}\n\\usepackage{graphicx}\n\n\\title{Tachyon-Tex: Asset Support}\n\\author{Antigravity AI}\n\\begin{document}\n\n\\maketitle\n\n\\section{Introduction}\nYou can now upload assets like images and include them in your LaTeX document.\n\n% Example: \\includegraphics[width=\\textwidth]{image.png}\n\n\\end{document}"
    }
};

let activeFile = "main.tex";
let socket;
let debounceTimer;
let activeFrame = frame1;
let inactiveFrame = frame2;
let currentUrl = null;
let errorTimeout;

// UI Initialization
function initUI() {
    editor.value = project.files[activeFile];
    renderFileList();
}

function renderFileList() {
    fileList.innerHTML = '';
    Object.keys(project.files).forEach(name => {
        const item = document.createElement('div');
        item.className = `file-item ${name === activeFile ? 'active' : ''}`;
        item.innerHTML = `<span>${name.endsWith('.tex') ? '📄' : '🖼️'} ${name}</span>`;
        item.onclick = () => switchFile(name);
        fileList.appendChild(item);
    });
}

function switchFile(name) {
    // Save current editor content
    if (activeFile.endsWith('.tex') || activeFile.endsWith('.sty') || activeFile.endsWith('.cls')) {
        project.files[activeFile] = editor.value;
    }

    activeFile = name;
    activeFileName.textContent = name;

    if (name.endsWith('.tex') || name.endsWith('.sty') || name.endsWith('.cls')) {
        editor.value = project.files[name];
        editor.disabled = false;
        editor.style.opacity = '1';
    } else {
        editor.value = "[Binary Data / Image]";
        editor.disabled = true;
        editor.style.opacity = '0.5';
    }

    renderFileList();
}

// Asset Management
function triggerAssetUpload() {
    assetUpload.click();
}

async function hdlAssetUpload(event) {
    const files = event.target.files;
    for (let i = 0; i < files.length; i++) {
        const file = files[i];
        const reader = new FileReader();

        if (file.name.endsWith('.tex') || file.name.endsWith('.sty') || file.name.endsWith('.cls')) {
            reader.onload = (e) => {
                project.files[file.name] = e.target.result;
                renderFileList();
                compile();
            };
            reader.readAsText(file);
        } else {
            // Convert to base64 for images/binaries
            reader.onload = (e) => {
                const b64 = e.target.result.split(',')[1];
                project.files[file.name] = b64;
                renderFileList();
                compile();
            };
            reader.readAsDataURL(file);
        }
    }
}

// WebSocket connection
function connect() {
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    socket = new WebSocket(`${proto}//${location.host}/ws`);

    socket.onopen = () => {
        statusDot.className = 'status-dot connected';
        statusText.textContent = 'Connected';
        compile();
    };

    socket.onmessage = (event) => {
        const data = JSON.parse(event.data);
        if (data.type === 'compile_success') {
            emptyState.style.display = 'none';
            renderPDF(data.pdf);
            compileTimeEl.textContent = data.compile_time_ms + 'ms';
            statusDot.className = 'status-dot connected';
            statusText.textContent = 'Synced';
        } else if (data.type === 'compile_error') {
            statusDot.className = 'status-dot';
            statusText.textContent = 'Error';
            showError('LaTeX Error: ' + data.error.substring(0, 300));
        }
    };

    socket.onclose = () => {
        statusDot.className = 'status-dot';
        statusText.textContent = 'Disconnected';
        setTimeout(connect, 2000);
    };
}

function renderPDF(base64) {
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
        bytes[i] = binary.charCodeAt(i);
    }
    const blob = new Blob([bytes], { type: 'application/pdf' });
    const url = URL.createObjectURL(blob);

    inactiveFrame.onload = () => {
        activeFrame.classList.add('hidden');
        inactiveFrame.classList.remove('hidden');
        [activeFrame, inactiveFrame] = [inactiveFrame, activeFrame];
        if (currentUrl) URL.revokeObjectURL(currentUrl);
        currentUrl = url;
    };
    inactiveFrame.src = url + '#toolbar=0&navpanes=0&scrollbar=1&view=FitH';
}

function compile() {
    if (socket && socket.readyState === WebSocket.OPEN) {
        // Sync active file content
        if (activeFile.endsWith('.tex') || activeFile.endsWith('.sty') || activeFile.endsWith('.cls')) {
            project.files[activeFile] = editor.value;
        }

        statusDot.className = 'status-dot compiling';
        statusText.textContent = 'Compiling...';

        // Send as Project JSON (v1.3)
        socket.send(JSON.stringify(project));
    }
}

function showError(message) {
    errorToast.textContent = message;
    errorToast.classList.add('visible');
    clearTimeout(errorTimeout);
    errorTimeout = setTimeout(() => { errorToast.classList.remove('visible'); }, 6000);
}

function exportPDF() {
    if (currentUrl) {
        const a = document.createElement('a');
        a.href = currentUrl;
        a.download = 'document.pdf';
        a.click();
    }
}

// Mobile Handlers
function toggleMobileView() {
    if (previewPanel.classList.contains('hidden-mobile')) {
        previewPanel.classList.remove('hidden-mobile');
        editorPanel.classList.add('hidden-mobile');
        document.getElementById('view-toggle').textContent = '📝';
        document.getElementById('files-toggle').style.display = 'none';
    } else {
        previewPanel.classList.add('hidden-mobile');
        editorPanel.classList.remove('hidden-mobile');
        document.getElementById('view-toggle').textContent = '👁️';
        document.getElementById('files-toggle').style.display = 'flex';
    }
}

function toggleFileTree() {
    fileTree.classList.toggle('visible');
}

// Event Listeners
editor.addEventListener('input', () => {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(compile, 450);
});

document.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 's') {
        e.preventDefault();
        exportPDF();
    }
});

window.addEventListener('load', () => {
    initUI();
    connect();

    // UI adaptation for desktop/mobile
    if (window.innerWidth <= 768) {
        document.getElementById('files-toggle').style.display = 'flex';
    }
});
