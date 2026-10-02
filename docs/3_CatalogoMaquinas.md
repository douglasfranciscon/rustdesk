# Catálogo automático de máquinas

## O que é

Cada máquina com o BR Remote **se cadastra sozinha** num catálogo no servidor (BR Suporte), e os
atendentes daquele catálogo entram nela **sem pedir senha ao cliente**. Cada máquina tem a sua
senha, que ninguém precisa conhecer: o servidor guarda só a "impressão" dela, o mesmo valor que a
lista de endereços já usa para entrar sem digitar.

| Login | Vê |
|---|---|
| `suporte@brproj` ("vê todos") | todas as máquinas, de todos os catálogos, mais as das listas pessoais de todos os atendentes, cada uma com a etiqueta do email de quem a tem (ou "sem dono") |
| login de revenda, ex. `suporte@invicta` (catálogo `invicta`) | as máquinas do catálogo dela, mais a lista pessoal |
| demais logins | só a lista pessoal, como sempre |

**O catálogo da máquina é a marca do app instalado:** o app da Invicta cadastra no catálogo
`invicta`; o app padrão, no catálogo geral (visto só pelo `suporte@brproj`).

O atendente **não precisa atualizar** o app: o catálogo chega pela mesma lista de endereços de
hoje. Quem precisa da versão nova é a **máquina do cliente**, que é quem se cadastra.

## O que o app faz (este repositório)

O serviço do app — que roda mesmo sem ninguém abrir a janela — a cada minuto:

1. **Garante uma senha permanente.** Se a máquina não tem, gera uma forte e aleatória, que ninguém
   vê. Se já tem (posta pelo cliente ou por um técnico), **mantém**.
2. **Manda o registro** ao servidor — ID, nome do computador, usuário do Windows, sistema, marca e
   a impressão da senha — **só quando algo mudou** desde o último registro aceito.
3. **Trocou a senha (ou apagou), registra de novo** em até um minuto: o catálogo nunca perde o acesso.
4. **Reenvia o registro uma vez por dia**, mesmo sem mudança. Uma máquina **apagada do catálogo**
   no portal volta sozinha em até 24 h se ainda estiver com o app (decisão do Douglas: "se a
   máquina voltar, ela se cadastra de novo"). Uma máquina formatada também volta assim, depois que
   o registro antigo for apagado no portal.

A senha em si nunca é lida: o app guarda a senha permanente já como impressão, e é essa impressão
que sobe. Ela abre **só aquela máquina**.

Se o servidor recusar ou não responder, o app tenta de novo depois de 5 minutos, e espaça até uma
tentativa por hora. Sem a rota no servidor, nada quebra: o app só não se cadastra.

A senha que o cliente vê na tela continua existindo — ele ainda pode dar acesso a quem quiser.

### Contrato com o servidor

```
POST {servidor de API}/api/maquina-registro      (sem login)
{ "id", "uuid", "hostname", "username", "os", "marca", "hash" }
→ 200 com o corpo exatamente  MACHINE_REGISTERED   (texto, não JSON)
```

⚠️ O sucesso é reconhecido **pelo corpo**, não pelo status: o núcleo do RustDesk (`post_request`)
descarta o código HTTP. Qualquer outra resposta é "não aceito". Mudar a resposta para JSON faria
as máquinas pararem de se cadastrar em silêncio.

`hash` = base64 do SHA256(senha permanente + salt da máquina), o mesmo formato do `hash` da lista
de endereços pessoal.

## O que fica com o servidor e o portal (apiGDe / BR GDe)

- o catálogo, com a impressão **cifrada**, e um registro por máquina amarrado à identidade dela
  (uma máquina formatada muda de identidade e precisa ser liberada no portal);
- a lista de endereços montada por login, com as etiquetas de email;
- os campos "Catálogo" e "vê todos" no cadastro do atendente;
- a página do portal para administrar os catálogos (renomear, remover, liberar).

A marca é **declarada pela própria máquina** — um instalador público não tem como provar nada. Por
decisão do Douglas, a máquina **entra direto** no catálogo que declarar, **sem aprovação**: "tem
cliente que não vai usar o portal", e uma fila deixaria o catálogo vazio justamente para esses. O
que sobra de defesa: a amarração pela identidade da máquina (ninguém toma o registro de uma
máquina já cadastrada) e a página do portal, onde se **vê e remove** depois o que não for legítimo.
Cadastrar uma máquina estranha não dá acesso à máquina de ninguém — só põe uma entrada a mais na
lista.

⚠️ **Se as máquinas não aparecerem no catálogo** com tudo pronto, suspeite primeiro das permissões
do banco (tabela nova nasce sem permissão para o usuário da API), não do app: o app não vê o
motivo da recusa e só tenta de novo, em silêncio. O log do serviço do app registra "machine
registration not accepted" com o começo da resposta.

## Onde isso está

| Arquivo | Papel |
|---|---|
| `src/hbbs_http/catalog.rs` | garante a senha e faz o registro |
| `src/hbbs_http/sync.rs` | chama o registro no laço do serviço |
| `src/brand.rs` | a marca do build no núcleo (o app das telas tem a sua em `flutter/lib/brand.dart`) |
| `res/brand/apply-brand.sh` | grava a marca em `src/brand.rs` num build de marca |
