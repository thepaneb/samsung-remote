# Controle Remoto Samsung 🎮📺

Aplicativo desktop (**Tauri 2 + Rust + TypeScript**) e **Android** para controlar TVs
Samsung (Tizen) de duas formas: **conexão direta pela rede local** (WebSocket seguro na
porta 8002 — rápido, lista real de apps, tecla Guia) e **nuvem do SmartThings** (para
ligar a TV do repouso e como reserva).

![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8DB)
![Rust stable](https://img.shields.io/badge/Rust-stable-orange)
![TypeScript](https://img.shields.io/badge/TypeScript-5+-3178C6)
![Plataforma](https://img.shields.io/badge/Windows%20%7C%20macOS%20%7C%20Linux%20%7C%20Android-blue)
[![Licença: MIT](https://img.shields.io/badge/Licen%C3%A7a-MIT-green)](LICENSE)
![Versão](https://img.shields.io/badge/vers%C3%A3o-0.1.0-blue)
![Node.js](https://img.shields.io/badge/Node.js-22+-339933)
![PRs bem-vindos](https://img.shields.io/badge/PRs-bem--vindos-brightgreen)

> Projeto **não oficial**, sem vínculo com a Samsung. Uso pessoal/educacional.

**Site:** <https://thepaneb.github.io/samsung-remote/>

## Funcionalidades

- ⏻ **Power** (ligar/desligar), volume e mudo
- 🕹️ **D-pad completo** (setas + OK), Voltar, Menu, Home e Sair
- 🗓 **Guia de canais** (`KEY_GUIDE` — disponível com a conexão direta)
- ⚡ **Conexão direta (rede)** — controle pela rede local, sem passar pela internet;
  com **pareamento feito no próprio app** (a TV mostra um pop-up para aceitar)
- 🔎 **Busca de TVs na rede** — descobre a TV automaticamente via SSDP (não precisa saber o IP);
  a lista mostra o nome e o modelo de cada TV encontrada
- 📱 **Lista real de apps instalados** — "Carregar da TV" traz os apps do seu modelo
  (com nomes) para adicionar aos atalhos com um toque
- ⌨️ **Digitar direto na TV** — envie texto para o campo em foco (busca do YouTube etc.)
- 📺 **Seletor de entradas** — troca direta para TV, HDMI1, HDMI2… com os nomes que você configurou
- 📡 **Canal +/−** e **play/pause**, retroceder e avançar
- 🔢 **Canal direto** — teclado numérico para ir a um canal (ex.: `590`, `15.1`)
- 🔍 **Buscar na TV** — envia um texto para a tela de busca da TV (teclado alfanumérico)
- 🎨 **Modos de imagem e som** — seletor com os modos que a TV suporta (marca o atual)
- ⏮⏭ **Faixa anterior/próxima** e ⏹ **parar**
- ⏺ **Gravação PVR** (alterna iniciar/parar; requer pendrive/HD na TV)
- 🌙 **Modo ambiente** (pressione Home na TV para sair)
- ⭐ **Favoritos** — atalhos configuráveis para canais ou apps (salvos no aparelho)
- ℹ️ **Sobre a TV** — modelo, sistema, firmware e suporte a ligar pela nuvem
- 📳 **Vibração** nos botões (Android)
- 🚀 **Atalhos de apps gerenciáveis** — catálogo com 20 apps (Netflix, YouTube, Prime Video,
  Disney+, Globoplay, Max, Telecine, Paramount+, Apple TV, Spotify, Deezer, TIDAL, Pluto TV,
  Plex, Twitch, TikTok, Internet, Galeria…), adição por ID personalizado,
  **reordenar (▲▼) e remover (×)**, tudo salvo no aparelho
- 📺 **Samsung TV Plus** — atalho que sintoniza o **canal 2000** e depois lembra o último
  canal ≥ 2000 usado
- ⌨️ **Atalhos de teclado**: setas = navegar • Enter = OK • Backspace = voltar • `+`/`−` volume •
  `M` mudo • `Espaço`/`P` play-pause • `H` início (desktop)
- 🔒 Tokens salvos localmente (você pode removê-los a qualquer momento no próprio app)

## Requisitos

- Windows 10/11 (WebView2 já vem no Windows 11), macOS ou Linux — ou Android
- [Node.js](https://nodejs.org) 22+ (LTS ativo) e [Rust](https://rustup.rs) (stable) para compilar
- Windows: Build Tools do Visual Studio (C++)

## Início rápido

```bash
npm install
npm run tauri dev
```

Gerar o instalador desktop:

```bash
npm run tauri build
```

Os artefatos ficam em `src-tauri/target/release/bundle` (`.msi`/`.exe` no Windows).

## Como usar

### 1. Conexão direta (recomendada)

1. No app, no cartão **"Conexão direta (rede)"**, clique em **Buscar TVs na rede** — o app
   encontra a TV automaticamente (via SSDP) e lista o **nome e o modelo** de cada uma. Escolha
   a sua: o IP é preenchido sozinho. (Se preferir, informe o IP manualmente: ele aparece na TV
   em *Configurações → Rede → Estado da rede*.)
2. Clique em **Parear com a TV**. A TV mostra um **pop-up** — escolha **Permitir** com o controle.
3. Pronto: o controle passa a ir pela rede (mais rápido), e ficam disponíveis o **Guia**,
   a **lista real de apps** e a **digitação direta**. O pareamento fica salvo no aparelho.

> Celular e TV precisam estar na **mesma rede Wi-Fi**.

### 2. Nuvem (SmartThings) — opcional, para ligar a TV do repouso

1. **Adicione a TV no app SmartThings do celular** (conta Samsung) — se ela já aparece lá, pronto.
2. Crie um **token pessoal** em <https://account.smartthings.com/tokens>:
   - Nome: `Controle Tauri`
   - Permissões: **Devices** → leitura (**`r:devices:*`**) e execução (**`x:devices:*`**)
3. Abra o app, cole o token no cartão **SmartThings** e clique em **Salvar**.
4. Escolha sua TV na lista e clique em **"Usar esta TV"**.

Com os dois configurados, o app usa a **rede local** no dia a dia e deixa a **nuvem**
como reserva (e para ligar a TV, que em repouso não tem rede ativa).

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
celular e instale (permitindo "fontes desconhecidas"). No aparelho, repita o processo do token
do SmartThings — a configuração é por dispositivo.

### Assinar o APK (sideload)

O build gera um APK **unsigned**; para instalar no Android é preciso assinar. Exemplo (Windows):

```powershell
# chave de assinatura (gerar uma vez; mantenha fora do Git)
keytool -genkeypair -v -keystore src-tauri/android-key/release.jks `
  -storepass samsungremote -keypass samsungremote -alias samsungremote `
  -keyalg RSA -keysize 2048 -validity 10950 -dname "CN=Samsung Remote, O=Personal, C=BR"

# alinhar e assinar (ferramentas em %LOCALAPPDATA%\Android\Sdk\build-tools\37.0.0)
zipalign -f -p 4 app-...-unsigned.apk aligned.apk
apksigner sign --ks src-tauri/android-key/release.jks `
  --ks-pass pass:samsungremote --key-pass pass:samsungremote `
  --out "Samsung Remote_0.1.0_aarch64.apk" aligned.apk
```

Para instalar via USB: ative a depuração USB e rode
`adb install -r "Samsung Remote_0.1.0_aarch64.apk"` (`adb` em `platform-tools`).

> Nota: o projeto usa **rustls** (TLS em Rust puro) em vez de native-tls para que a mesma base
> de código compile para desktop e Android sem depender de OpenSSL do sistema.

## Como funciona

### Conexão direta (rede local)

O app fala **direto com a TV** por um WebSocket TLS na porta **8002** (a mesma API usada pelo
app oficial e pela biblioteca `samsungtvws`):

| Item | Detalhe |
| --- | --- |
| Descoberta | SSDP (M-SEARCH em `239.255.255.250:1900`); nome/modelo lidos em `http://<ip>:8000/api/v2/` |
| URL | `wss://<ip>:8002/api/v2/channels/samsung.remote.control?name=<base64>&token=<token>` |
| Pareamento | conexão sem token → pop-up na TV → token salvo em `local.json` |
| Teclas | `ms.remote.control` com `KEY_*` (inclui `KEY_GUIDE`, `KEY_CH_LIST`, números…) |
| Lista de apps | evento `ed.installedApp.get` (nomes + IDs reais do seu modelo) |
| Abrir app | `POST /api/v2/applications/<id>` na API local; em modelos sem REST, evento `ed.apps.launch` como reserva |
| Digitar texto | `SendInputString` (base64) + `SendInputEnd` |
| Canal direto | dígitos `KEY_0`…`KEY_9` + `KEY_ENTER` (na TV ao vivo; com um app aberto, use a nuvem) |
| TLS | certificado autoassinado da TV é aceito (é a própria TV na LAN) |

### Nuvem (SmartThings)

O app usa a **API pública do SmartThings** (`https://api.smartthings.com/v1`) com seu token pessoal:

| Ação | Capacidade / comando |
| --- | --- |
| Ligar/desligar | `switch` (`on`/`off`, alternando pelo estado) |
| Volume | `audioVolume` (`volumeUp`/`volumeDown`) |
| Mudo | `samsungvd.remoteControl.send MUTE` (ou `audioMute`) |
| D-pad, OK, Voltar, Menu, Home, Sair | `samsungvd.remoteControl.send UP/DOWN/LEFT/RIGHT/OK/BACK/MENU/HOME/EXIT` |
| Fonte | `samsungvd.mediaInputSource.setInputSource` (lista com os nomes das entradas) |
| Canal | `tvChannel` (`channelUp`/`channelDown`/`setTvChannel`) |
| Buscar na TV | `custom.tvsearch.search` (texto) |
| Modos de imagem/som | `custom.picturemode` / `custom.soundmode` (`setPictureMode`/`setSoundMode`) |
| Faixa anterior/próxima | `mediaTrackControl` (`previousTrack`/`nextTrack`) |
| Parar | `mediaPlayback.stop` (ou tecla `STOP`) |
| Gravação PVR | `custom.recording` (`recordStart`/`recordStop`) |
| Modo ambiente | `samsungvd.ambient.setAmbientOn` |
| Play/Pause, ⏪, ⏩ | `samsungvd.remoteControl.send PLAY_BACK/REWIND/FF` (ou `mediaPlayback`) |
| Abrir app | `custom.launchapp.launchApp` com o appId |

As teclas são habilitadas automaticamente conforme as **capacidades que a sua TV expõe** —
teclas não suportadas aparecem apagadas. O mapeamento de cada capacidade para o componente do
dispositivo é descoberto ao selecionar a TV e salvo em `smartthings.json` (pasta de dados do app).

## Solução de problemas

<details>
<summary>Clique para expandir</summary>

| Problema | O que fazer |
| --- | --- |
| "Token inválido ou sem permissão" | Gere novo token marcando **Devices** com leitura **e** execução (`r:devices:*`, `x:devices:*`). |
| Busca não encontra a TV | Confirme que a TV está **ligada** e na mesma rede Wi-Fi (SSDP não atravessa roteadores/VLANs separadas). Firewalls podem bloquear o UDP 1900 — libere o app ou informe o IP manualmente. |
| TV não aparece na lista | Confirme que ela está adicionada no app SmartThings. A lista mostra primeiro os dispositivos com cara de TV; se nada bater, mostra todos. |
| Pop-up de pareamento não aparece | Confirme o IP da TV e que o celular/PC estão na **mesma rede**; tente novamente — o pop-up aparece na primeira conexão de cada nome de app. |
| "A TV não autorizou a conexão" | Você recusou (ou o pop-up expirou). Clique em **Parear com a TV** de novo e escolha **Permitir**. |
| Atalho abre o app errado | IDs variam por região/ano — use **"Carregar da TV"** no diálogo de apps para pegar os IDs reais do seu modelo, ou o campo de ID personalizado. |
| Botões de apps não têm ação na TV | O app abre apps pela **API REST local** e mostra o resultado: se a TV recusar, aparece um aviso — em geral o ID do atalho não existe neste modelo. Use **"Carregar da TV"**. |
| Tecla apagada | A TV não expõe a capacidade correspondente (nuvem) e a conexão direta não está ativa. Pareie pela rede para habilitar as teclas locais. |
| Guia/Canais | Funciona com a **conexão direta** (`KEY_GUIDE`). Pela nuvem não existe: o canal de controle valida as teclas e aceita apenas UP, DOWN, LEFT, RIGHT, OK, BACK, EXIT, MENU, HOME, MUTE, PLAY, PAUSE, STOP, REWIND, FF, PLAY_BACK, SOURCE. |
| Comando não faz efeito | Verifique se a TV está ligada; com a rede ativa o comando é local, senão passa pela nuvem do SmartThings. |
| TV Plus/canal não muda (mas o app diz "enviado") | Com um app aberto, os dígitos enviados pela rede são consumidos pelo app em primeiro plano. Por isso a troca de canal prioriza a nuvem (`setTvChannel`); sem nuvem configurada, volte à TV ao vivo antes de digitar o canal. |
| TV desligada: o controle não responde | Em repouso a TV desliga a rede — use **Power** (nuvem) para ligar; depois os comandos voltam a ser locais. |
| O indicador de entrada atual não acompanha trocas feitas direto na TV | Nesses modelos a nuvem congela o estado de entrada/energia. O app mantém memória própria das últimas ações feitas **por ele** e sincroniza quando a nuvem atualiza. |
| Power alterna ao contrário (1º clique não faz o esperado) | Mesma causa acima — o app alterna com base no último estado conhecido; um clique extra resolve. |

</details>

## Estrutura do projeto

```
index.html            # layout do controle
src/
├── main.ts           # lógica da interface (comandos via invoke)
└── styles.css        # visual
src-tauri/
├── tauri.conf.json   # productName, identifier, janela
├── capabilities/     # permissões do Tauri
├── examples/         # testes manuais contra TV real
│   ├── local-test.rs
│   ├── channel-test.rs
│   ├── launch-test.rs
│   └── inputs-test.rs
└── src/
    ├── lib.rs         # bootstrap do Tauri e registro dos comandos
    ├── commands.rs    # comandos expostos ao frontend
    ├── discovery.rs   # descoberta de TVs na rede (SSDP) + nome/modelo
    ├── local.rs       # conexão direta (WebSocket TLS na 8002, teclas, apps)
    └── smartthings.rs # cliente da API SmartThings
```

Para quem vai mexer no código, veja também o **[AGENTS.md](AGENTS.md)** (guia para
agentes de código: comandos, arquitetura e convenções).

## Privacidade e segurança

- O token do SmartThings e o token de pareamento ficam **só no seu aparelho**
  (pasta de dados do app no SO), nunca saem para terceiros além da Samsung/SmartThings.
- A conexão direta fala com a TV na **rede local**; o certificado autoassinado da TV
  é aceito porque o par é a própria TV na LAN.
- A chave de assinatura do APK (`src-tauri/android-key/`) e quaisquer `.env`/`*.jks`
  **não são versionados** (ver `.gitignore`). Antes do primeiro `push`, confira com
  `git status` que nenhum segredo está na lista.

## Contribuindo

1. Abra uma issue descrevendo o problema ou a melhoria (informe modelo/ano da TV).
2. Fork + branch a partir de `master`, commits pequenos em pt-BR.
3. Valide com `npx tsc --noEmit` e `cargo check --manifest-path src-tauri/Cargo.toml`.
4. Abra o PR explicando o que foi testado (TV real? qual modelo? rede, nuvem ou ambos?).

## Licença

Distribuído sob a licença [MIT](LICENSE).

## Nota histórica

A primeira versão controlava a TV apenas localmente (SSDP + WebSocket); quando o pareamento
passou a ser recusado pela TV, o app foi reescrito só com a **nuvem**. Depois de validar o
pareamento pela porta **8002** (o mesmo caminho usado pela biblioteca `samsungtvws`), a
**conexão direta voltou** — agora implementada nativamente em Rust (TLS com rustls, sem
dependências externas para parear), com a nuvem mantida como reserva e para ligar a TV.
