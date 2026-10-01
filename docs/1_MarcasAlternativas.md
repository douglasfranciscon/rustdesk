# Gerar o app com a logo de outra marca

O mesmo aplicativo pode sair com outro conjunto de logos e ícones — para um cliente que queira o
suporte remoto com a cara dele. A escolha é feita **na hora de rodar o build**, num campo a mais
do "Run workflow": nada de commit, nada de trocar arquivo no repositório.

> **Muda a arte, a cor, o site e um sufixo no título.** Além das logos, o build pinta o app com
> a cor da marca (ver "A cor da marca"), mostra o nome da pasta no título da janela ("BR Remote -
> Invicta") e aponta o "Website" para o site da marca (ver "O site da marca"). Nome do programa
> ("BR Remote"), serviço, pasta de instalação, servidor e chave continuam os mesmos em todas as
> marcas. Ver "O que a marca **não** troca", no fim.

## Como está organizado

As artes **não ficam neste repositório**, que é público. Ficam no repositório privado
[br-suporte-secrets](https://github.com/douglasfranciscon/br-suporte-secrets), em `marcas/`, uma
pasta por marca:

```
marcas/
  0modelo/       <- gabarito: tamanhos e arquivos que toda marca tem
  brremote/      <- a marca padrão BR Remote (copie esta para criar uma nova)
  invicta/       <- marca Invicta Tecnologia
```

Dentro de cada pasta, **os arquivos de arte ficam no mesmo caminho que têm neste repositório**,
e na raiz ficam o site e a cor do cliente:

```
marcas/invicta/site.txt
marcas/invicta/cor.txt
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
3. Troque, na raiz da pasta, o `site.txt` pelo site do cliente e o `cor.txt` pela cor principal
   do logo dele (ver abaixo). Os dois vieram do `brremote` com os valores da BR PROJ.
4. Commit e push no `br-suporte-secrets`.

⚠️ O **nome da pasta aparece no título da janela**, com a primeira letra maiúscula: a pasta
`invicta` vira "BR Remote - Invicta". A pasta `brremote` é a exceção — é a marca padrão, e o
título fica só "BR Remote".

## O site da marca

Um arquivo **`site.txt`** na raiz da pasta da marca (`marcas/invicta/site.txt`, ao lado do
`LEIAME.md`) diz para onde vai o link "Website" da tela Sobre — no Windows e no Android. Uma linha
só, com o domínio:

```
www.invictatecnologia.com.br
```

`https://` e barra no fim são aceitos e descartados; BOM e quebra de linha do Windows também.
**Sem** `site.txt`, o link continua em `www.brproj.com.br`. Com um `site.txt` que não pareça um
site (vazio, duas linhas, aspas, espaço), o build **falha** no passo "Apply brand" mostrando o
conteúdo do arquivo — melhor do que sair um instalador com link quebrado.

A faixa larga da tela Sobre (`flutter/assets/logo.png`) traz o domínio desenhado na imagem, e o
`gerar_marca.py` o tira do `Criar/site.txt` — e grava o mesmo `site.txt` dentro da pasta da
marca, para os dois dizerem sempre o mesmo site.

## A cor da marca

Um arquivo **`cor.txt`** na raiz da pasta da marca, ao lado do `site.txt`, com a cor principal do
logo do cliente em hexadecimal. Uma linha só:

```
#0070C8
```

O `#` é opcional; minúscula, BOM e quebra de linha do Windows são aceitos. **Sem** `cor.txt`, o
app sai no verde da BR PROJ. Com um `cor.txt` que não seja uma cor de 6 dígitos (texto, `#07C`,
duas cores), o build **falha** no "Apply brand".

O app não usa uma cor só, usa cinco tons dela, e o build deriva os outros quatro da cor dada
(`res/brand/brand_colors.py`), mantendo o matiz e mudando só a luminosidade:

| tom | onde aparece | como sai |
|---|---|---|
| a cor | ID, detalhes de marca | a cor do `cor.txt`, sem mudança |
| destaque | botões, chaves, abas, seleção (texto branco em cima) | a própria cor, se já tiver contraste 4,5:1 com branco; senão, escurecida até ter |
| escuro | texto e ícone de marca sobre fundo claro | escurecida até 5:1 contra o fundo claro |
| ID da janela de conexão | o ID de quem está conectando | igual ao escuro |
| fundo claro | fundos claros tingidos de leve com a cor | a cor bem clara e quase cinza |

Por isso, **escolha a cor que representa o cliente**, não a mais legível: se ela for clara demais
para botão, o build escurece só o botão. O azul da Invicta (`#0070C8`) já tem 5:1 e sai igual; o
verde-menta do mesmo logo (`#00D078`) viraria um verde-petróleo nos botões.

Não mudam com a marca, de propósito: os verdes de "deu certo" do RustDesk (mensagem de sucesso,
regra de senha cumprida), que significam "ok" e não "marca", e as cores da interface antiga do
Windows 32 bits (Sciter), que são outras.

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
   (`replaced  res/icon.ico`), o sufixo do título, o site e a cor gravados (`title  invicta`,
   `website  www.invictatecnologia.com.br`, `color  #0070C8` com os cinco tons) e, no fim, os
   caminhos de marca que **não** foram trocados (`kept  ...`) — é o lembrete do que ainda falta
   desenhar.
2. No instalador baixado, confira **primeiro a versão** (Propriedades → Detalhes, `1.4.9.<run>`)
   para ter certeza de que é o build novo, e depois o ícone do `.exe`, o ícone na bandeja, a
   logo na tela inicial, a cor dos botões e do ID, o título da janela e o link "Website" em
   Configurações → Sobre.

## Testar a pasta antes de gastar um build

O mesmo script que o CI usa roda na sua máquina, contra uma **cópia** do repositório:

```bash
bash res/brand/apply-brand.sh /caminho/br-suporte-secrets/marcas/invicta /caminho/copia-do-repo
```

Ele recusa e não copia nada se algum arquivo da pasta apontar para um caminho que não existe no
repositório (quase sempre um caminho digitado errado, que passaria batido e faria o build sair
com a arte antiga).

## O que a marca **não** troca

Nome do app, textos e servidor são iguais para todas as marcas — o sufixo no título é só
o que se vê na barra da janela; o nome interno, o serviço e a pasta de instalação continuam
`BRRemote`. Duas consequências:

- Os instaladores têm o mesmo nome de arquivo e o mesmo código de produto: instalar a marca B por
  cima da A é **atualização no lugar**, não duas instalações convivendo na mesma máquina.
- No Android, o `applicationId` é o mesmo — vale a mesma coisa.

Se um dia for preciso que duas marcas convivam, aí o nome do app também tem que variar por marca
(hoje ele é fixado no CI, em `.github/workflows/flutter-build.yml`, pelo `sed` do `APP_NAME`).

## Onde isso está implementado

| Arquivo | Papel |
|---|---|
| `res/brand/apply-brand.sh` | copia a pasta da marca por cima da árvore e valida os caminhos; grava o nome da pasta, o `site.txt` e os tons do `cor.txt` no `brand.dart` |
| `res/brand/brand_colors.py` | deriva os cinco tons a partir da cor do `cor.txt` |
| `flutter/lib/brand.dart` | sufixo do título, site do "Website" e os cinco tons (lidos pelo `MyTheme`); o que está commitado é o build padrão |
| `res/brand/paths.txt` | lista dos caminhos de marca conhecidos (gera os avisos de "kept") |
| `.github/actions/apply-brand/action.yml` | baixa do repositório privado **só a pasta da marca** (o keystore e as senhas, na raiz dele, nem chegam à máquina do build) e chama o script |
| `.github/workflows/flutter-nightly.yml` | o campo "Marca" do "Run workflow" e a release por marca |
| `.github/workflows/flutter-build.yml` | passa a marca aos 4 jobs que geram instalador (Windows flutter, Windows sciter, Android e Android universal) |
