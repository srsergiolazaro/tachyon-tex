import os
import time
import zipfile
import io
import requests

def create_zip(tex_content):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, 'w') as zf:
        zf.writestr("main.tex", tex_content)
    buf.seek(0)
    return buf

def run_test(name, tex_content):
    print(f"Running test: {name}...")
    zip_buf = create_zip(tex_content)
    
    start_time = time.time()
    try:
        response = requests.post(
            "http://localhost:8080/compile",
            files={"file": ("test.zip", zip_buf, "application/zip")}
        )
        end_time = time.time()
        
        if response.status_code == 200:
            compile_time_ms = response.headers.get("X-Compile-Time-Ms", "Unknown")
            total_time_ms = int((end_time - start_time) * 1000)
            print(f"✅ Success! Compile time: {compile_time_ms}ms, Total RTT: {total_time_ms}ms")
            return compile_time_ms, total_time_ms
        else:
            print(f"❌ Failed: {response.status_code} - {response.text}")
            return None, None
    except Exception as e:
        print(f"❌ Error: {e}")
        return None, None

def create_complex_zip():
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, 'w') as zf:
        zf.writestr("main.tex", r"""
\documentclass{article}
\usepackage{amsmath}
\begin{document}
\tableofcontents
\section{Introduction}
As shown in Section \ref{sec:math}, LaTeX is fast.
\section{Math}
\label{sec:math}
\begin{equation}
    e^{i\pi} + 1 = 0
\end{equation}
\end{document}
""")
    buf.seek(0)
    return buf

def main():
    # ... existing code ...
    print("Waiting for server to be ready...")
    # Add actual tests here after server is confirmed running
    # result1 = run_test("Simple Doc", warmup_tex)
    # result2 = run_test("TikZ Doc", tikz_tex)
    # result3 = run_test("Complex Doc", create_complex_zip().read())

if __name__ == "__main__":
    main()
