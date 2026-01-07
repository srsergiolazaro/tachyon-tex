Write-Host "🚀 Building the Tachyon-Tex orbital engine..." -ForegroundColor Cyan
docker build -t tachyon-tex .

if ($LASTEXITCODE -eq 0) {
    Write-Host "✨ Build successful. Launching on :8080" -ForegroundColor Green
    Write-Host "💡 Using RAM disk for Zero-I/O compilation." -ForegroundColor Yellow
    # Note: --tmpfs is only for Linux containers inside Docker Desktop
    docker run -p 8080:8080 --tmpfs /dev/shm:rw,size=512m tachyon-tex
} else {
    Write-Host "❌ Build failed. Please check technical logs." -ForegroundColor Red
}
