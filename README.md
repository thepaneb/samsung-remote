# Controle Remoto Samsung 🎮📺

Aplicativo desktop (**Tauri 2 + Rust + TypeScript**) para controlar TVs Samsung pela
**nuvem do SmartThings** — funciona mesmo quando a TV recusa o pareamento de controle
local (WebSocket).

![status](https://img.shields.io/badge/Tauri-2-24C8DB) ![status](https://img.shields.io/badge/Rust-stable-orange) ![status](https://img.shields.io/badge/TypeScript-5+-3178C6)

## Funcionalidades

- ⏻ **Power** (ligar/desligar), volume e mudo
- 🕹️ **D-pad completo** (setas + OK), Voltar, Menu, Home e Sair
- 📺 **Seletor de entradas** — troca direta para TV, HDMI1, HDMI2… com os nomes que você configurou
- 📡 **Canal +/−** e **play/pause**, retroceder e avançar
- 🚀 **Atalhos de apps** (Netflix, YouTube, Prime Video, Disney+, Globoplay, Max, Spotify, Internet)
  + campo para abrir qualquer app por ID
- ⌨️ **Atalhos de teclado**: setas = navegar • Enter = OK • Backspace = voltar • `+`/`−` volume •
  `M` mudo • `Espaço`/`P` play-pause • `H` início
- 🔒 Token salvo localmente (você pode removê-lo a qualquer momento no próprio app)

## Requisitos

- Windows 10/11 (WebView2 já vem no Windows 11), macOS ou Linux
- [Node.js](https://nodejs.org) 18+ e [Rust](https://rustup.rs) (stable) para compilar
- Windows: Build Tools do Visual Studio (C++)

## Como usar

1. **Adicione a TV no app SmartThings do celular** (conta Samsung) — se ela já aparece lá, pronto.
2. Crie um **token pessoal** em <https://account.smartthings.com/tokens>:
   - Nome: `Controle Tauri`
   - Permissões: **Devices** → leitura (**`r:devices:*`**) e execução (**`x:devices:*`**)
3. Abra o app, cole o token no cartão **SmartThings** e clique em **Salvar**.
4. Escolha sua TV na lista e clique em **“Usar esta TV”** — o controle está pronto.

## Executar em desenvolvimento

```bash
npm install
npm run tauri dev
```

## Gerar o instalador

```bash
npm run tauri build
```

Os artefatos ficam em `src-tauri/target/release/bundle` (`.msi`/`.exe` no Windows).

## Android (APK)

O app também compila para **Android** (Tauri 2 mobile). Pré-requisitos:

- **JDK 17+** (`JAVA_HOME`)
- **Android SDK**: platform-tools, `platforms;android-34`, `build-tools;34.0.0` (`ANDROID_HOME`)
- **NDK** r26+ (ex.: `ndk;28.2.13676358`) com `NDK_HOME` apontando para a pasta do NDK
- Alvos Rust: `rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android`

```bash
npm run tauri android init                      # gera src-tauri/gen/android (já versionado)
npm run tauri android build -- --apk --target aarch64
```

O APK sai em `src-tauri/gen/android/app/build/outputs/apk/arm64/release/`. Transfira para o
celular e instale (permitindo “fontes desconhecidas”). No aparelho, repita o processo do token
do SmartThings — a configuração é por dispositivo.

## Como funciona

O app usa a **API pública do SmartThings** (`https://api.smartthings.com/v1`) com seu token pessoal:

| Ação | Capacidade / comando |
| --- | --- |
| Ligar/desligar | `switch` (`on`/`off`, alternando pelo estado) |
| Volume | `audioVolume` (`volumeUp`/`volumeDown`) |
| Mudo | `samsungvd.remoteControl.send MUTE` (ou `audioMute`) |
| D-pad, OK, Voltar, Menu, Home, Sair | `samsungvd.remoteControl.send UP/DOWN/LEFT/RIGHT/OK/BACK/MENU/HOME/EXIT` |
| Fonte | `samsungvd.mediaInputSource.setInputSource` (lista com os nomes das entradas) |
| Canal | `tvChannel` (`channelUp`/`channelDown`) |
| Play/Pause, ⏪, ⏩ | `samsungvd.remoteControl.send PLAY_BACK/REWIND/FF` (ou `mediaPlayback`) |
| Abrir app | `custom.launchapp.launchApp` com o appId |

As teclas são habilitadas automaticamente conforme as **capacidades que a sua TV expõe** —
teclas não suportadas aparecem apagadas. O mapeamento de cada capacidade para o componente do
dispositivo é descoberto ao selecionar a TV e salvo em `smartthings.json` (pasta de dados do app).

## Solução de problemas

| Problema | O que fazer |
| --- | --- |
| “Token inválido ou sem permissão” | Gere novo token marcando **Devices** com leitura **e** execução (`r:devices:*`, `x:devices:*`). |
| TV não aparece na lista | Confirme que ela está adicionada no app SmartThings. A lista mostra primeiro os dispositivos com cara de TV; se nada bater, mostra todos. |
| Atalho abre o app errado | IDs variam por região/ano — use o campo de ID personalizado com o código correto do seu modelo. |
| Tecla apagada | A TV não expõe a capacidade correspondente no SmartThings; não há como enviar essa tecla pela nuvem. |
| Comando não faz efeito | Verifique se a TV está ligada e conectada à internet (o caminho é pela nuvem do SmartThings). |
| O indicador de entrada atual não acompanha trocas feitas direto na TV | Nesses modelos a nuvem congela o estado de entrada/energia. O app mantém memória própria das últimas ações feitas **por ele** e sincroniza quando a nuvem atualiza. |
| Power alterna ao contrário (1º clique não faz o esperado) | Mesma causa acima — o app alterna com base no último estado conhecido; um clique extra resolve. |

## Estrutura do projeto

```
src-tauri/src/
├── lib.rs         # bootstrap do Tauri e registro dos comandos
├── smartthings.rs # cliente da API SmartThings (auth, comandos, entradas)
└── commands.rs    # comandos expostos ao frontend
src/
├── main.ts        # lógica da interface
└── styles.css     # visual
index.html         # layout do controle
```

## Nota histórica

A primeira versão deste projeto controlava a TV **localmente** (descoberta SSDP, WebSocket na
porta 8001/8002, Wake-on-LAN), mas a TV de testes passou a **recusar todo pareamento local**
(estado travado do serviço da TV). O projeto foi então reescrito com escopo reduzido para o
**modo nuvem**, que funciona de forma confiável. O código local continua no histórico do Git.

## Aviso

Projeto **não oficial**, sem vínculo com a Samsung. Uso pessoal/educacional.
