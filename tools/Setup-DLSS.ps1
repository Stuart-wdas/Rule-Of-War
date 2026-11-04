# Optional build dependencies. Running the compiled game needs only its DLL.
$ErrorActionPreference = 'Stop'
$rowRoot = Split-Path $PSScriptRoot -Parent
$rowTools = Join-Path $rowRoot '.data/dlss-tools'
New-Item -ItemType Directory -Force $rowTools | Out-Null
if (!(Test-Path "$rowTools/DLSS/include/nvsdk_ngx.h")) {
    git clone --depth 1 --branch v310.4.0 --filter=blob:none --sparse https://github.com/NVIDIA/DLSS.git "$rowTools/DLSS"
    if ($LASTEXITCODE -ne 0) {throw 'DLSS SDK download failed'}
    git -C "$rowTools/DLSS" sparse-checkout set include lib/Windows_x86_64/x64 lib/Windows_x86_64/rel doc
    if ($LASTEXITCODE -ne 0) {throw 'DLSS SDK checkout failed'}
}
if (!(Test-Path "$rowTools/Vulkan-Headers/include/vulkan/vulkan.h")) {
    git clone --depth 1 https://github.com/KhronosGroup/Vulkan-Headers.git "$rowTools/Vulkan-Headers"
    if ($LASTEXITCODE -ne 0) {throw 'Vulkan headers download failed'}
}
if (!(Test-Path "$rowTools/clang-runtime/clang/native/libclang.dll")) {
    python -m pip install --target "$rowTools/clang-runtime" libclang
    if ($LASTEXITCODE -ne 0) {throw 'libclang download failed'}
}
Write-Host 'DLSS build dependencies ready. Cargo uses the paths in .cargo/config.toml.'
