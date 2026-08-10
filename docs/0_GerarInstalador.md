# Como gerar o instalador (BR Remote)

Guia rápido pra gerar o `.exe`/`.msi` do Windows (e opcionalmente o `.apk` do Android) a partir deste fork, usando o GitHub Actions — não precisa instalar nada localmente.

## 1. Pré-requisitos (já configurados, só conferir se algo mudar)

No fork, em **Settings → Secrets and variables → Actions**, devem existir estes secrets:

| Secret | O que é |
|---|---|
| `RENDEZVOUS_SERVER` | Domínio/IP do servidor hbbs |
| `RS_PUB_KEY` | Chave pública do hbbs (base64, arquivo `id_ed25519.pub`) |
| `API_SERVER` | URL da API (se usar recursos de conta/address book) |
| `ANDROID_SIGNING_KEY` | Keystore Android em base64, pra assinar o `.apk` (ver "Assinatura do Android" abaixo) |
| `ANDROID_ALIAS` | Alias da chave dentro do keystore |
| `ANDROID_KEY_STORE_PASSWORD` | Senha do keystore |
| `ANDROID_KEY_PASSWORD` | Senha da chave |

E em **Settings → Actions → General → Workflow permissions**, precisa estar marcado **"Read and write permissions"** (sem isso, a etapa que publica a release falha com erro 403).

### Valores atuais dos servidores

O GitHub não deixa reler um secret depois de salvo — só a data em que foi alterado. Então ficam registrados aqui (são hostnames públicos, não são segredo; o `RS_PUB_KEY` também é público por definição, mas fica só no secret pra não convidar cópia):

```
RENDEZVOUS_SERVER=brremoteserver.brproj.com.br
API_SERVER=https://brsuporteapi.brproj.com.br
```

**São duas máquinas diferentes** — é o erro mais fácil de cometer aqui:

| | `RENDEZVOUS_SERVER` | `API_SERVER` |
|---|---|---|
| Valor | `brremoteserver.brproj.com.br` | `https://brsuporteapi.brproj.com.br` |
| Aponta pra | `br-bi-rustdesk.4yw6mk.easypanel.host` (31.97.94.206) | `apibrgde-…azurewebsites.net` (191.232.176.16) |
| Formato | host puro — **sem** `https://`, **sem** `:21116` | URL completa **com** `https://`, sem barra no fim |
| Protocolo | TCP/UDP 21116 (rendezvous) e 21117 (relay) | HTTPS |
| Pra que serve | conectar as máquinas (ID, NAT traversal, relay) | login, address book, grupos, logs |
| Obrigatório? | **sim** — sem ele o cliente não conecta em nada | não — sem ele o app deriva `host:21114` do rendezvous |
| Onde é injetado no build | `libs/hbb_common/src/config.rs` (`RENDEZVOUS_SERVERS`) | `src/common.rs` (fallback de `get_api_server_`) |

O `RS_PUB_KEY` pertence ao **hbbs**, não à API — ao trocar de servidor de rendezvous, os dois têm que mudar juntos, senão o cliente recusa a conexão. Trocar só o nome DNS apontando pra mesma máquina não exige chave nova.

Pra conferir rapidamente se os valores estão sãos:

```powershell
# rendezvous: as duas portas têm que responder
Test-NetConnection brremoteserver.brproj.com.br -Port 21116
Test-NetConnection brremoteserver.brproj.com.br -Port 21117

# api: tem que devolver JSON (e não o HTML de um site)
Invoke-WebRequest https://brsuporteapi.brproj.com.br/api/login -Method Post -Body '{}' -ContentType application/json
#   -> 200 {"error":"Usuário ou senha inválidos"}
```

Cuidado com nomes parecidos que **não** servem como rendezvous: `brremote.brproj.com.br`, `brsuporte.brproj.site` e `rustdeskapi.brproj.site` são todos alias do app web no Azure (191.232.176.16) e não escutam na 21116. Os nomes antigos `rustdesk.brproj.site` (rendezvous) e `rustdeskapi.brproj.site` (api) foram usados até jul/2026.

### Assinatura do Android

O keystore usado pra assinar o `.apk` (RSA 2048, autoassinado, válido até 2056) **não fica neste repositório** (que é público) — ele está guardado no repositório privado [br-suporte-secrets](https://github.com/douglasfranciscon/br-suporte-secrets), junto com um `README-secrets.txt` com o alias e as duas senhas.

Pra (re)cadastrar os 4 secrets acima:
1. Clone/acesse o `br-suporte-secrets` (privado).
2. `ANDROID_SIGNING_KEY` = conteúdo de `brsuporte-release.jks.base64.txt`.
3. Os outros 3 valores (alias + 2 senhas) estão em `README-secrets.txt`.

**Nunca** commitar o keystore ou as senhas neste repositório (rustdesk) — só no `br-suporte-secrets`. Perder o keystore significa que ninguém que já instalou o BR Remote consegue receber uma atualização in-place nunca mais (só desinstalando e reinstalando do zero).

Sem esses 4 secrets configurados, o workflow ainda funciona, mas publica o `.apk` sem assinatura (`Publish unsigned apk package`).

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
  - `BRRemote-<versão>-x86_64.exe` → executável autoextraível, arquivo único
  - `BRRemote-<versão>-x86_64.msi` → instalador Windows
  - (se Android rodou) `BRRemote-<versão>-<arch>.apk` — vai pra Releases também (assinado se os 4 secrets do Android estiverem configurados, senão sem assinatura)

## 5. Testar

- Instalar/rodar o `.msi` ou `.exe` numa máquina de teste
- Confirmar nome "BR Remote" e ícone corretos
- Gerar um ID e testar conexão real com o servidor próprio

## 6. Assinar o Windows (.exe / .msi) manualmente

O `.exe`/`.msi` que sai do CI **não é assinado** — o mecanismo de assinatura do workflow (`res/job.py`, secrets `SIGN_BASE_URL`/`SIGN_SECRET_KEY`) espera um servidor de assinatura HTTP próprio, que não existe aqui. Além disso, o certificado de code-signing fica num token/HSM de hardware, que uma máquina virtual do GitHub Actions não consegue acessar — então essa assinatura precisa ser feita manualmente, na máquina onde o token está conectado.

Passo a passo, depois de baixar `BRRemote-<versão>-x86_64.exe`/`.msi` da aba Releases:

1. Conecte o token/HSM do certificado.
2. Abra um terminal com o `signtool.exe` no PATH (vem com o Windows SDK).
3. Rode, pra cada arquivo:
   ```
   signtool sign /a /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 "BRRemote-<versão>-x86_64.exe"
   signtool sign /a /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 "BRRemote-<versão>-x86_64.msi"
   ```
   - `/a` escolhe automaticamente o certificado de assinatura de código disponível (vai pedir a senha/PIN do token).
   - Troque a URL do `/tr` pelo servidor de timestamp da sua CA, se for diferente.
4. Confirme a assinatura: botão direito no arquivo → Propriedades → aba "Assinaturas Digitais".
5. Substitua os arquivos não assinados na Release (ou distribua os assinados separadamente).
