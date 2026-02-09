param(
    [switch]$Windows
)

if ($Windows) {
    Write-Host "🚀 Cross-compiling Tachyon-Tex for Windows..." -ForegroundColor Cyan
    docker build -f Dockerfile.cross -t tachyon-tex-cross .
    
    if ($LASTEXITCODE -eq 0) {
        Write-Host "✨ Compilation successful (MSVC). Extracting tachyon-tex.exe..." -ForegroundColor Green
        
        # Create a temporary container to extract the binary
        $containerId = docker create tachyon-tex-cross
        docker cp "${containerId}:/tachyon-tex.exe" .\tachyon-tex.exe
        docker rm $containerId
        
        if (Test-Path .\tachyon-tex.exe) {
            Write-Host "✅ Binary extracted! Launching server..." -ForegroundColor Green
            .\tachyon-tex.exe
        } else {
            Write-Host "❌ Failed to extract tachyon-tex.exe" -ForegroundColor Red
        }
    } else {
        Write-Host "❌ Windows Build failed. Please check technical logs." -ForegroundColor Red
    }
} else {
    Write-Host "🚀 Building the Tachyon-Tex orbital engine (Linux)..." -ForegroundColor Cyan
    docker build -t tachyon-tex .

    if ($LASTEXITCODE -eq 0) {
        Write-Host "✨ Build successful. Launching on :8080" -ForegroundColor Green
        Write-Host "💡 Using RAM disk for Zero-I/O compilation." -ForegroundColor Yellow
        # Note: --tmpfs is only for Linux containers inside Docker Desktop
        docker run -p 8080:8080 --tmpfs /dev/shm:rw,size=512m tachyon-tex
    } else {
        Write-Host "❌ Build failed. Please check technical logs." -ForegroundColor Red
    }
}
