<#
CUSTOM BRANDING: assina todos os .exe e .dll da pasta crua do build e gera o zip
para a etapa 2 ("Empacotar Windows com DLLs assinadas"). Ver docs/2_AssinarDLLs.md.

Uso (no computador com o token do certificado):

    powershell -ExecutionPolicy Bypass -File res\sign\assinar-pasta.ps1 -Pasta C:\temp\rustdesk-unsigned-windows-x86_64

- Pula os arquivos que já têm assinatura válida (DLLs que vêm assinadas pelo
  fabricante); assina o resto numa única chamada ao signtool, então o token pede
  o PIN uma vez só (depende da configuração do token).
- Confere no fim que tudo ficou assinado e grava assinado-x86_64.zip ao lado da
  pasta (ou onde -Zip mandar).
#>
param(
    [Parameter(Mandatory = $true)] [string] $Pasta,
    [string] $Zip = "",
    [string] $Timestamp = "http://timestamp.digicert.com"
)

$ErrorActionPreference = 'Stop'

# signtool: no PATH, ou no Windows SDK.
$signtool = (Get-Command signtool.exe -ErrorAction SilentlyContinue).Source
if (-not $signtool) {
    $signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match '\\x64\\' } |
        Sort-Object FullName -Descending |
        Select-Object -First 1 -ExpandProperty FullName
}
if (-not $signtool) {
    throw "signtool.exe não encontrado. Instale o Windows SDK ou ponha o signtool no PATH."
}
Write-Host "signtool: $signtool"

$Pasta = (Resolve-Path $Pasta).Path
if (-not (Test-Path (Join-Path $Pasta 'BRRemote.exe'))) {
    throw "BRRemote.exe não está em $Pasta. Aponte para a pasta descompactada do artefato rustdesk-unsigned-windows-x86_64."
}

function Relativo($caminho) { $caminho.Substring($Pasta.Length + 1) }

$arquivos = @(Get-ChildItem $Pasta -Recurse -File -Include *.exe, *.dll)
$assinar = @()
foreach ($f in $arquivos) {
    $s = Get-AuthenticodeSignature $f.FullName
    if ($s.Status -eq 'Valid') {
        Write-Host ("já assinado   {0}  ({1})" -f (Relativo $f.FullName), $s.SignerCertificate.Subject)
    } else {
        $assinar += $f.FullName
    }
}

if ($assinar.Count -gt 0) {
    Write-Host ""
    Write-Host "assinando $($assinar.Count) de $($arquivos.Count) arquivo(s)..."
    & $signtool sign /a /fd SHA256 /tr $Timestamp /td SHA256 @assinar
    if ($LASTEXITCODE -ne 0) { throw "signtool falhou (código $LASTEXITCODE)." }
}

# Conferência final: a etapa 2 recusa a pasta se sobrar um arquivo sem assinatura.
$falta = @($arquivos | Where-Object { (Get-AuthenticodeSignature $_.FullName).Status -ne 'Valid' })
if ($falta.Count -gt 0) {
    $falta | ForEach-Object { Write-Host "SEM ASSINATURA VÁLIDA: $(Relativo $_.FullName)" }
    throw "$($falta.Count) arquivo(s) continuam sem assinatura válida."
}
Write-Host "$($arquivos.Count) exe/dll, todos assinados."

if (-not $Zip) { $Zip = Join-Path (Split-Path $Pasta -Parent) 'assinado-x86_64.zip' }
if (Test-Path $Zip) { Remove-Item $Zip }
Compress-Archive -Path (Join-Path $Pasta '*') -DestinationPath $Zip
Write-Host ""
Write-Host "pronto: $Zip"
Write-Host "Anexe esse zip à release do build (nightly, ou nightly-<marca>) e rode a etapa 2."
