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
| `BRAND_ASSETS_TOKEN` | Só para gerar com a logo de outro cliente — ver [1_MarcasAlternativas.md](1_MarcasAlternativas.md) |

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

Nenhum build roda sozinho: nem à noite, nem a cada commit (o "CI" e o "Full Flutter CI" do
upstream também ficaram só no botão). O workflow fica **habilitado** e só espera o clique — se
um dia aparecer como desabilitado, foi desligado à mão no GitHub, não pelo repositório.

1. Vá em **Actions** no fork (`github.com/douglasfranciscon/rustdesk/actions`)
2. Na lista à esquerda, clique em **"Flutter Nightly Build"**
3. Clique no botão **"Run workflow"** (canto direito)
4. Marque a branch `master`
5. Se quiser gerar **só o Windows** (mais rápido, não espera Android/Linux/macOS/iOS/web): marque a caixinha **"Build only Windows..."**
   - Deixe desmarcada se também quiser o `.apk` do Android
6. **"Marca: pasta de logos a usar"**: deixe **vazio** para o BR Remote de sempre. Preenchendo
   (ex.: `invicta`), o build sai com a logo e a cor daquela marca, o site dela no "Website" do
   Sobre e o nome da pasta no título ("BR Remote - Invicta"), e publica numa release separada — ver
   [1_MarcasAlternativas.md](1_MarcasAlternativas.md)
7. Clique em **Run workflow**

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
  - (se "Build only Windows" ficou desmarcado) `BRRemote-<versão>-x86-sciter.exe` → o Windows **32 bits**, para máquina que não roda o de 64. É a interface **antiga (Sciter)**, não a Flutter: leva servidor, chave, nome, senha fixa e ícones, mas nenhuma das telas personalizadas (abas, Sobre, título, logos internas)

Se o build foi gerado **com marca**, os arquivos não vão para a release `nightly`, e sim para
uma release própria da marca — `nightly-invicta`, por exemplo. Os nomes dos arquivos são os
mesmos (`BRRemote-…`), e é justamente por isso que a release é separada.

O `<versão>` dos arquivos do Windows tem quatro partes: `1.4.9.<n>`, onde `<n>` é o
número da run do Actions (contador automático, sobe a cada build). Serve pra saber
qual build é qual — o mesmo número aparece em Propriedades → Detalhes do `.exe` e na
versão registrada em Programas e Recursos depois de instalar o `.msi`.

## 5. Testar

- Instalar/rodar o `.msi` ou `.exe` numa máquina de teste
- **Antes de testar qualquer coisa**, conferir o número da versão em Propriedades →
  Detalhes do arquivo. É a única forma de garantir que a máquina está rodando o build
  que você acabou de gerar, e não um anterior
- Confirmar nome "BR Remote" e ícone corretos
- Gerar um ID e testar conexão real com o servidor próprio

⚠️ Ao instalar um `.msi` novo por cima de um instalado **antes** do contador existir: a
versão registrada antigamente era um número enorme (minutos desde 1970), então o
Windows Installer pode enxergar o pacote novo como mais antigo e recusar o upgrade.
Nesse caso, desinstalar antes. O `.exe` portátil não tem esse problema.

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

## 7. Avisar os clientes de versão nova (Windows)

Ao abrir, o app no Windows consulta o servidor de API (`GET /api/aviso-app`, sem login — quem abre
o app é o cliente, que nunca loga) e pode mostrar duas janelas:

- **Mensagem:** um texto seu, com botão OK, **toda vez que o app abrir** enquanto houver mensagem.
- **Versão nova:** se a versão do app for **anterior ao corte** (a versão mínima que você exige),
  pergunta se quer baixar. **Sim** abre o download no navegador; **Não** fecha. Nada é instalado
  sozinho, e a pergunta volta a cada abertura enquanto a versão continuar abaixo do corte.

Os três valores (corte, pasta do download e mensagem) ficam nas **App Settings** do servidor de API
no Azure, não neste repositório — os nomes exatos estão com o back (apiGDe, `brsuporte`). Vazio
desliga: sem mensagem não há janela; sem corte ou sem pasta não há oferta de download. Se o
servidor estiver fora do ar, o app abre normalmente, sem aviso nenhum.

### A versão é a data do build

Cada build grava a **data em que foi gerado**, no horário de Brasília, como `AAAA.MM.DD`
(`2026.10.15`). Ela aparece no Sobre, ao lado da versão do RustDesk — `Versão: 1.4.9 (2026.10.15)` —
e no log do passo "Patch custom rendezvous server" (`BR version: 2026.10.15`). É a mesma para todas
as marcas geradas no mesmo dia e não tem nada a ver com o `1.4.9.<run>` das propriedades do `.exe`.

### A versão anunciada é um corte, não "a mais nova"

O app avisa quando a data **dele** é **anterior** ao corte. Por isso **publicar um build não exige
mexer na App Setting**: você pode lançar várias versões em silêncio, deixar os primeiros adotarem,
e só **subir o corte** quando quiser mover todo mundo — é aí, e só aí, que os de baixo são avisados.
Ficar com o corte parado por vários lançamentos é o normal; o que não pode é esquecer de subi-lo
quando quiser que os clientes atualizem.

Ao subir, use a data do build **mais antigo** que você quer considerar em dia: gerou o padrão em
15/10 e a Invicta em 16/10, corte em `2026.10.15` — senão o padrão de 15/10 ficaria pedindo para
baixar ele mesmo. O download leva sempre ao arquivo que estiver na pasta, que pode ser mais novo
que o corte.

⚠️ **Arquivo primeiro, corte depois — e o corte nunca acima da data de algum arquivo da pasta.**
O app não sabe a data do que está na pasta antes de baixar, e o servidor não olha a pasta. Se o
corte passar da data de um dos `.exe` (o padrão ou o de alguma marca), quem baixa esse arquivo
continua abaixo do corte, e o aviso volta a cada abertura, para sempre. O sintoma chega como
"atualizei e ele pede de novo", longe da causa.

O corte é **um só**, para todas as marcas. E não serve para voltar atrás: um corte mais velho não
avisa ninguém. Build ruim se corrige recompilando o commit bom — ele sai com a data do dia — e
subindo o corte para ela.

⚠️ O corte tem que estar **exatamente** em `AAAA.MM.DD`, com zeros (`2026.10.05`, não
`2026.10.5`). Fora desse formato o app não oferece nada — um erro de digitação não dispara aviso
em todo mundo, mas também não avisa ninguém. Build local (fora do GitHub) não tem data e nunca avisa.

### Publicar uma leva

1. Gere os builds (padrão e cada marca) e assine os `.exe` (passo 6).
2. Suba na pasta do download, **sem versão no nome**:
   - `BRRemote-x86_64.exe` — o padrão
   - `BRRemote-x86_64_<pasta>.exe` — cada marca (ex.: `BRRemote-x86_64_invicta.exe`)
3. Pronto: quem baixar agora pega a leva nova. Só quando quiser **avisar** quem ficou para trás,
   e **depois** que todos os `.exe` da leva estiverem na pasta, suba o corte nas App Settings
   (produção **e** o slot `apibrgdedeploy`) até, no máximo, a data do build mais antigo da leva. A pasta do download
   (ex.: `https://brprojbackupapp.s3.sa-east-1.amazonaws.com/BRRemote/`) só se configura uma vez.

O app monta o nome do arquivo sozinho: pasta + `BRRemote-x86_64` + `_<pasta da marca>` (só em build
de marca) + `.exe`. Esse nome mora no código (`flutter/lib/common/widgets/app_notice.dart`); se um
dia os arquivos mudarem de nome, só um app novo passa a achá-los.
