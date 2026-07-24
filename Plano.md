# Plano: Customizar cliente RustDesk (nome, ícone, servidor)

Repositório: fork de `rustdesk/rustdesk` (código do cliente).
Servidor: já existente (hbbs/hbbr rodando), não precisa mexer nele.

## 0. Pré-requisitos
- [ ] Fork feito em `https://github.com/SEU-USUARIO/rustdesk`
- [ ] Repositório clonado localmente (GitHub Desktop ou `git clone --recursive https://github.com/SEU-USUARIO/rustdesk`)
- [ ] Pasta aberta no VS Code
- [ ] Ter em mãos: endereço do servidor (rendezvous), chave pública (`id_ed25519.pub` do hbbs), endereço do relay (se separado)

## 1. Configurar servidor no código
Arquivo: `libs/hbb_common/src/config.rs`

Localizar e editar:
```rust
pub const DEFAULT_RENDEZVOUS_SERVER: &str = ""; // -> seu domínio/IP
pub const DEFAULT_RS_PUB_KEY: &str = "";        // -> chave pública base64
pub const DEFAULT_RELAY_SERVER: &str = "";      // -> se usar relay separado
```
- [ ] Substituir os 3 valores
- [ ] Salvar

## 2. Trocar nome do app
Arquivos a revisar (buscar por "RustDesk" no VS Code, Ctrl+Shift+F):
- [ ] `src/lang/*.rs` (strings da interface)
- [ ] `Cargo.toml` (campo `name`/`description`, se aplicável)
- [ ] Recursos do Windows (título do instalador, gerado pelo `build.py`)
- [ ] Nome exibido na janela/título (procurar constantes tipo `APP_NAME`)

## 3. Trocar ícone/logo
Pastas:
- [ ] `res/` — ícones `.ico` usados no Windows (build/instalador)
- [ ] `flutter/assets/` — ícone/logo usados na interface Flutter (splash, About, etc.)

Substituir os arquivos existentes pelos seus, mantendo o mesmo nome de arquivo e formato (`.ico`, `.png`, `.svg`).

## 4. Configurar Secrets no GitHub (build na nuvem, sem instalar nada local)
No fork, em Settings → Secrets and variables → Actions:
- [ ] `RENDEZVOUS_SERVER`
- [ ] `RS_PUB_KEY`
- [ ] `RELAY_SERVER` (se aplicável)

## 5. Ajustar workflow do GitHub Actions
Arquivo: `.github/workflows/flutter-build.yml` (ou criar um novo `.yml` dedicado)
- [ ] Confirmar que o workflow lê os Secrets acima e substitui `config.rs` antes do build
- [ ] Adicionar passo de build: `python3 build.py --portable --hwcodec --flutter --vram --skip-portable-pack`
- [ ] Adicionar passo de upload do artifact (`.exe` gerado)

## 6. Commit e push
- [ ] `git add .`
- [ ] `git commit -m "Custom branding e servidor"`
- [ ] `git push`

## 7. Rodar e baixar o build
- [ ] Ir na aba **Actions** do fork no GitHub
- [ ] Conferir se o workflow rodou sem erro
- [ ] Baixar o `.exe` gerado (artifact da execução)

## 8. Testar
- [ ] Instalar/rodar o `.exe` em uma máquina de teste
- [ ] Confirmar nome e ícone corretos
- [ ] Confirmar que conecta no seu servidor (gerar ID e tentar conexão)

---
Dúvidas comuns:
- Servidor (hbbs/hbbr) fica em repo separado (`rustdesk/rustdesk-server`), não precisa fork dele.
- Build roda 100% no GitHub Actions (nuvem) — VS Code é só pra editar os arquivos e dar commit/push.