# Controle Remoto Samsung (Tauri)

Aplicativo desktop para controlar **TVs Samsung (Tizen, 2016+)** pela rede local.
Feito com **Tauri 2** (Rust no backend, TypeScript no frontend).

![status](https://img.shields.io/badge/Tauri-2-24C8DB) ![status](https://img.shields.io/badge/Rust-stable-orange) ![status](https://img.shields.io/badge/TypeScript-5+-3178C6)

## Funcionalidades

- 🔍 **Descoberta automática** de TVs na rede (SSDP) e adição manual por IP
- 🤝 **Pareamento com token** — aceite uma vez na TV e a conexão passa a ser direta
- 🕹️ **Controle completo**: power, volume, canal, D-pad, Home, Voltar, Menu, Guia, Info, Ferramentas, mídia (⏪ ⏯ ⏩), teclas coloridas e teclado numérico (inclui PRÉ e +100)
- ⌨️ **Envio de texto** para o campo aberto na TV (busca do YouTube/Netflix etc.)
- 📺 **Lista de apps** da TV com ícones, para abrir com um clique
- ⏻ **Wake-on-LAN** para ligar a TV desligada pelo MAC
- ⌨️ **Atalhos de teclado**: setas, Enter, Backspace, `+`/`−`, `M`, `Espaço`, `P`, `H`

## Requisitos

- Windows 10/11 (WebView2 já vem no Windows 11), macOS ou Linux
- [Node.js](https://nodejs.org) 18+
- [Rust](https://rustup.rs) (canal stable)
- Windows: Build Tools do Visual Studio (C++)

## Executar em modo de desenvolvimento

```bash
npm install
npm run tauri dev
```

## Gerar o instalador

```bash
npm run tauri build
```

Os artefatos ficam em `src-tauri/target/release/bundle` (`.msi`/`.exe` no Windows).

## Como usar

1. Ligue a TV e deixe-a na **mesma rede** do computador.
2. Abra o app e clique em **“Buscar TVs”** (ou digite o IP e clique em “Adicionar”).
3. Clique em **“Conectar”** na TV encontrada.
4. Na TV aparece um aviso do tipo *“Permitir que ‘Controle Tauri’ se conecte?”* —
   aceite com o **controle físico**. O token fica salvo e as próximas conexões são automáticas.

### Ligar a TV desligada (Wake-on-LAN)

- A TV precisa já ter sido pareada uma vez (para guardarmos o **MAC**).
- Na TV: **Configurações → Geral → Rede → Configurações avançadas → “Ligar com celular” (Power On with Mobile)** = ativado. O nome varia conforme o modelo/ano.
- Funciona de forma mais confiável com **cabo de rede**; via Wi-Fi depende do modelo.
- No app, clique no botão **⏻** ao lado da TV na lista.

## Solução de problemas

| Problema | O que fazer |
| --- | --- |
| Nenhuma TV encontrada | Confirme que a TV está ligada e na mesma rede; adicione pelo IP manualmente (TV: Configurações → Geral → Rede → Status da rede). |
| “A TV recusou a conexão” | Clique em **✕ (Esquecer)** e conecte de novo para gerar um novo pedido de permissão na TV. |
| Firewall | Libere as portas **8000/8001/8002** da TV e UDP **1900** (SSDP) no Windows. |
| Texto não é digitado | O campo de texto precisa estar aberto/focado na TV; alguns apps bloqueiam entrada externa. |
| Teclas coloridas/PRÉ/+100 sem efeito | Isso varia entre modelos — é a TV que decide interpretar cada código. |

## Como funciona (protocolo)

- **Descoberta:** SSDP `M-SEARCH` em `239.255.255.250:1900`, alvo `urn:samsung.com:device:RemoteControlReceiver:1`.
- **Informações:** `GET http://IP:8000/api/v2/` (nome, modelo, MAC, `PowerState`).
- **Controle:** WebSocket `ws://IP:8001/api/v2/channels/samsung.remote.control?name=<base64>`, com fallback para `wss://IP:8002` (certificado autoassinado).
- **Teclas:** `{"method":"ms.remote.control","params":{"Cmd":"Click","DataOfCmd":"KEY_...","TypeOfRemote":"SendRemoteKey"}}`.
- **Texto:** `SendInputString` (com o broadcast `custom.remote.textReceived` antes, exigido por alguns modelos).
- **Apps:** `GET http://IP:8001/api/v2/applications` + `ms.channel.emit` / `ed.apps.launch`.
- **Pareamento:** a TV devolve o token em `ms.channel.connect`; ele é salvo em `devices.json` na pasta de dados do app.

## Estrutura do projeto

```
src-tauri/src/
├── lib.rs        # bootstrap do Tauri e registro dos comandos
├── ssdp.rs       # descoberta de TVs (UDP multicast)
├── device.rs     # informações da TV via HTTP
├── tv.rs         # WebSocket: handshake, teclas, texto, loop da sessão
├── wol.rs        # Wake-on-LAN (magic packet)
├── store.rs      # persistência de TVs/tokens (devices.json)
└── commands.rs   # comandos expostos ao frontend
src/
├── main.ts       # lógica da interface
└── styles.css    # visual
index.html        # layout do controle
```

## Aviso

Projeto **não oficial**, sem vínculo com a Samsung. Uso pessoal/educacional.
