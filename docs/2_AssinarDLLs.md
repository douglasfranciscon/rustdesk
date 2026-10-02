# Assinar também as DLLs (Windows 64 bits)

## Quando usar

No Windows 11 com o **Controle Inteligente de Aplicativos** ligado, o Windows bloqueia qualquer
DLL sem assinatura digital. O sintoma é uma janela "**Imagem Incorreta**" ao abrir o app, citando
uma DLL (por exemplo `url_launcher_windows_plugin.dll`) e o status `0xc0e90002`.

Isso acontece porque o processo normal ([0_GerarInstalador.md](0_GerarInstalador.md), passo 6)
assina só o `.exe` e o `.msi` **de fora**. As DLLs que vão **dentro** deles são empacotadas pelo
GitHub antes, sem assinatura.

## Por que em duas etapas

O certificado fica no token físico, que o GitHub não alcança. Então a assinatura das DLLs é feita
no meio do caminho, na sua máquina:

1. **Etapa 1** — o build de sempre compila e deixa a pasta crua (exe + DLLs), sem empacotar.
2. **Você** assina tudo da pasta com um comando e devolve o zip.
3. **Etapa 2** — um segundo workflow empacota a pasta assinada no `.exe` portátil e no `.msi`.
4. **Você** assina o `.exe` e o `.msi` de fora, como já faz hoje.

O workflow "Flutter Nightly Build" **não muda**; a etapa 2 é um workflow à parte.

## Passo a passo

1. **Rode o build normal** — Actions → "Flutter Nightly Build" → Run workflow, com a marca se for
   o caso ("Build only Windows" marcado basta). **Anote o número da execução**: o `N` que aparece
   como `#N` na lista e nos arquivos `BRRemote-1.4.9.N-…`.

2. **Baixe a pasta crua** — na página dessa execução, em **Artifacts**, baixe
   `rustdesk-unsigned-windows-x86_64` e descompacte, por exemplo em
   `C:\temp\rustdesk-unsigned-windows-x86_64`.

3. **Assine a pasta**, com o token conectado, a partir da pasta do repositório:

   ```powershell
   powershell -ExecutionPolicy Bypass -File res\sign\assinar-pasta.ps1 -Pasta C:\temp\rustdesk-unsigned-windows-x86_64
   ```

   O script assina todo `.exe` e `.dll` que ainda não tem assinatura válida (pula as DLLs que já vêm
   assinadas pelo fabricante), numa chamada só ao `signtool` — o token pede o PIN uma vez,
   dependendo da configuração dele. No fim confere tudo e grava `C:\temp\assinado-x86_64.zip`.

4. **Anexe o zip à release do build** — no GitHub, aba **Releases** → `nightly` (ou
   `nightly-<marca>`, ex. `nightly-invicta`) → ícone de lápis (editar) → arraste o
   `assinado-x86_64.zip` em "Attach binaries" → **Update release**.

   A release é pública, mas as DLLs assinadas são as mesmas que você distribui no instalador, então
   não expõe nada novo. Pode apagar o zip da release depois da etapa 2.

5. **Rode a etapa 2** — Actions → "**Empacotar Windows com DLLs assinadas**" → Run workflow:
   - **build**: o `N` do passo 1 (só o número, ex. `97`);
   - **marca**: a mesma do passo 1 (vazio para o BR Remote padrão);
   - **zip**: `assinado-x86_64.zip` (já vem preenchido).

   Ele **recusa** a pasta se sobrar algum `.exe`/`.dll` sem assinatura válida, e lista no log o
   estado de cada arquivo.

6. **Baixe o resultado** — na página da execução da etapa 2, em Artifacts, baixe
   `BRRemote-1.4.9.N-x86_64-dlls-assinadas`: tem o `BRRemote-1.4.9.N-x86_64.exe` e o `.msi`, com
   as DLLs de dentro assinadas.

7. **Assine o `.exe` e o `.msi`** como no passo 6 de [0_GerarInstalador.md](0_GerarInstalador.md)
   e distribua (para o aviso de versão nova, suba na pasta do download como
   `BRRemote-x86_64.exe` / `BRRemote-x86_64_<marca>.exe` — ver a seção 7 do mesmo guia).

## Bom saber

- **Use os arquivos da etapa 2**, não os da release: a release continua com os da etapa 1, com as
  DLLs de dentro sem assinatura.
- A etapa 2 usa o número `N` que você informou nos nomes dos arquivos, no `.exe` portátil e na
  versão que o `.msi` registra — por isso tem que ser o da etapa 1, não o da execução da etapa 2.
- Só **64 bits (x86_64)**. O Windows ARM e o 32 bits não passam por aqui.
- Cada marca é uma leva separada: etapa 1 com a marca, zip na release dela, etapa 2 com a marca.

## Onde isso está

| Arquivo | Papel |
|---|---|
| `res/sign/assinar-pasta.ps1` | assina a pasta crua e gera o zip (roda na sua máquina) |
| `.github/workflows/empacotar-assinado.yml` | etapa 2: baixa o zip da release, confere as assinaturas, empacota `.exe` e `.msi` |
