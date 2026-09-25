# Gerar o app com a logo de outra marca

O mesmo aplicativo pode sair com outro conjunto de logos e ícones — para um cliente que queira o
suporte remoto com a cara dele. A escolha é feita **na hora de rodar o build**, num campo a mais
do "Run workflow": nada de commit, nada de trocar arquivo no repositório.

> **Só a arte muda.** Nome do programa ("BR Remote"), cores do app, servidor e chave continuam os
> mesmos em todas as marcas. Ver "O que a marca **não** troca", no fim.

## Como está organizado

As artes **não ficam neste repositório**, que é público. Ficam no repositório privado
[br-suporte-secrets](https://github.com/douglasfranciscon/br-suporte-secrets), em `marcas/`, uma
pasta por marca:

```
marcas/
  brremote/      <- a arte padrão de hoje (referência de tamanhos)
  invicta/       <- por enquanto, cópia da brremote (placeholder)
```

Dentro de cada pasta, **os arquivos ficam no mesmo caminho que têm neste repositório**:

```
marcas/invicta/res/icon.ico
marcas/invicta/flutter/assets/logo.png
marcas/invicta/flutter/android/app/src/main/res/mipmap-xxxhdpi/ic_launcher.png
```

É isso que dispensa qualquer lista de mapeamento: a pasta **é** o mapa. O build copia cada
arquivo para o caminho correspondente e compila.

A pasta não precisa estar completa: o que ela não traz continua saindo com a arte commitada aqui.
O `LEIAME.md` de cada marca traz a **tabela de tamanhos** de cada arquivo (é o que se manda para
quem for desenhar), e `res/brand/paths.txt` lista todos os caminhos de marca conhecidos.

## Criar uma marca nova

1. No `br-suporte-secrets`, copie `marcas/brremote` para `marcas/<codinome>`.
2. Substitua os arquivos pela arte do cliente, **mantendo caminho e tamanho** (a tabela está no
   `LEIAME.md` que veio junto). Pode trocar só alguns.
3. Commit e push no `br-suporte-secrets`.

⚠️ **Use codinome, não o nome do cliente.** O valor digitado no "Run workflow" aparece na página
da execução, que é pública neste fork.

## Rodar o build com a marca

Igual ao build normal (ver [0_GerarInstalador.md](0_GerarInstalador.md)), com um campo a mais:

1. **Actions → "Flutter Nightly Build" → "Run workflow"**
2. Preencha **"Marca: pasta de logos a usar"** com o nome da pasta (ex.: `invicta`).
   Deixando vazio, sai o BR Remote padrão de sempre.
3. Marque "Build only Windows" se não precisar do `.apk`.

Os arquivos saem na aba **Releases**, numa release própria da marca: **`nightly-<marca>`**
(ex.: `nightly-invicta`). O build sem marca continua publicando em `nightly`.

A release é separada porque os nomes de arquivo **não** mudam — todas as marcas geram
`BRRemote-<versão>-x86_64.exe`. Se todas publicassem em `nightly`, a última sobrescreveria as
anteriores.

## O pré-requisito (uma vez só)

Em **Settings → Secrets and variables → Actions** do fork, tem que existir:

| Secret | O que é |
|---|---|
| `BRAND_ASSETS_TOKEN` | Token do GitHub com permissão de **leitura** no `br-suporte-secrets` (fine-grained PAT, *Contents: Read*, só nesse repositório) |

Sem ele, um build **com** marca falha de propósito, com a mensagem
`brand '<marca>' was requested but the assets token is empty` — melhor falhar do que gerar um
instalador com a arte errada sem ninguém perceber. Um build **sem** marca não usa o token e
continua funcionando normalmente.

⚠️ PAT fine-grained vence (12 meses, no máximo). Quando vencer, o sintoma é o passo "Fetch brand
assets" falhando com 404/403 no clone.

## Conferir o resultado

1. Na execução, abra o passo **"Apply brand"**: ele lista cada arquivo substituído
   (`replaced  res/icon.ico`) e, no fim, os caminhos de marca que **não** foram trocados
   (`kept  ...`) — é o lembrete do que ainda falta desenhar.
2. No instalador baixado, confira **primeiro a versão** (Propriedades → Detalhes, `1.4.9.<run>`)
   para ter certeza de que é o build novo, e depois o ícone do `.exe`, o ícone na bandeja e a
   logo na tela inicial.

## Testar a pasta antes de gastar um build

O mesmo script que o CI usa roda na sua máquina, contra uma **cópia** do repositório:

```bash
bash res/brand/apply-brand.sh /caminho/br-suporte-secrets/marcas/invicta /caminho/copia-do-repo
```

Ele recusa e não copia nada se algum arquivo da pasta apontar para um caminho que não existe no
repositório (quase sempre um caminho digitado errado, que passaria batido e faria o build sair
com a arte antiga).

## O que a marca **não** troca

Nome do app, textos, cores e servidor são iguais para todas as marcas. Duas consequências:

- Os instaladores têm o mesmo nome de arquivo e o mesmo código de produto: instalar a marca B por
  cima da A é **atualização no lugar**, não duas instalações convivendo na mesma máquina.
- No Android, o `applicationId` é o mesmo — vale a mesma coisa.

Se um dia for preciso que duas marcas convivam, aí o nome do app também tem que variar por marca
(hoje ele é fixado no CI, em `.github/workflows/flutter-build.yml`, pelo `sed` do `APP_NAME`).

## Onde isso está implementado

| Arquivo | Papel |
|---|---|
| `res/brand/apply-brand.sh` | copia a pasta da marca por cima da árvore e valida os caminhos |
| `res/brand/paths.txt` | lista dos caminhos de marca conhecidos (gera os avisos de "kept") |
| `.github/actions/apply-brand/action.yml` | baixa a pasta do repositório privado e chama o script |
| `.github/workflows/flutter-nightly.yml` | o campo "Marca" do "Run workflow" e a release por marca |
| `.github/workflows/flutter-build.yml` | passa a marca aos 4 jobs que geram instalador (Windows flutter, Windows sciter, Android e Android universal) |
