# Como gerar o instalador (BRSuporte)

Guia rápido pra gerar o `.exe`/`.msi` do Windows (e opcionalmente o `.apk` do Android) a partir deste fork, usando o GitHub Actions — não precisa instalar nada localmente.

## 1. Pré-requisitos (já configurados, só conferir se algo mudar)

No fork, em **Settings → Secrets and variables → Actions**, devem existir 3 secrets:

| Secret | O que é |
|---|---|
| `RENDEZVOUS_SERVER` | Domínio/IP do servidor hbbs |
| `RS_PUB_KEY` | Chave pública do hbbs (base64, arquivo `id_ed25519.pub`) |
| `API_SERVER` | URL da API (se usar recursos de conta/address book) |

E em **Settings → Actions → General → Workflow permissions**, precisa estar marcado **"Read and write permissions"** (sem isso, a etapa que publica a release falha com erro 403).

## 2. Rodar o workflow

1. Vá em **Actions** no fork (`github.com/douglasfranciscon/rustdesk/actions`)
2. Na lista à esquerda, clique em **"Flutter Nightly Build"**
3. Clique no botão **"Run workflow"** (canto direito)
4. Marque a branch `master`
5. Se quiser gerar **só o Windows** (mais rápido, não espera Android/Linux/macOS/iOS/web): marque a caixinha **"Build only Windows..."**
   - Deixe desmarcada se também quiser o `.apk` do Android
6. Clique em **Run workflow**

## 3. Acompanhar

A execução aparece na lista de runs da aba Actions. Clique nela pra ver o progresso de cada job. Um build completo (todas as plataformas) demora bem mais que só Windows.

Se algum job falhar com mensagens tipo **"Too many retries"** / **"Cache service responded with 400"**: é uma instabilidade passageira do próprio GitHub Actions, não é problema do código. Use o botão **"Re-run failed jobs"** na página da run.

## 4. Baixar o resultado

Quando terminar, tem **dois lugares** pra olhar — não confunda os dois:

- **Aba "Artifacts"** (embaixo da página da run) → `rustdesk-unsigned-windows-x86_64.zip` (e aarch64) — é só a pasta crua do build (exe + dlls soltos), útil pra debug, **não é o instalador**.
- **Aba "Releases"** do repositório (`github.com/douglasfranciscon/rustdesk/releases`) → uma release pré-lançamento chamada **"nightly"** com os arquivos de verdade pra distribuir:
  - `rustdesk-<versão>-x86_64.exe` → executável autoextraível, arquivo único
  - `rustdesk-<versão>-x86_64.msi` → instalador Windows
  - (se Android rodou) o `.apk` fica na aba Artifacts do job Android

## 5. Testar

- Instalar/rodar o `.msi` ou `.exe` numa máquina de teste
- Confirmar nome "BRSuporte" e ícone corretos
- Gerar um ID e testar conexão real com o servidor próprio
