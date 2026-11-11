# AGENTS.md — Guia para agentes de código

> Idioma da UI e da documentação: **pt-BR**. Comentários e mensagens de commit em pt-BR.

## O que é este projeto

App desktop (Windows/macOS/Linux) e Android — **Tauri 2 + Rust + TypeScript/Vite** —
para controlar TVs Samsung (Tizen) por dois caminhos:

1. **Conexão direta (rede local)** — WebSocket TLS na porta `8002` (`wss://<ip>:8002/...`).
   Rápida, com pareamento via pop-up na TV, tecla `KEY_GUIDE`, lista real de apps e
   digitação direta. É o caminho preferido.
2. **Nuvem SmartThings** — API `https://api.smartthings.com/v1` com token pessoal.
   Reserva + ligar a TV do repouso (em repouso a TV desliga a rede local).

## Comandos

```bash
npm install            # instalar dependências do frontend
npm run dev            # só o frontend (Vite, porta 1420)
npm run tauri dev      # app desktop em desenvolvimento
npm run build          # tsc + vite build (frontend)
npm run tauri build    # instalador desktop (artefatos em src-tauri/target/release/bundle)

npx tsc --noEmit       # checagem de tipos do frontend
cargo check --manifest-path src-tauri/Cargo.toml   # checagem do backend Rust
```

Android (requer JDK 17+, Android SDK 34, NDK r26+ e targets Rust Android):

```bash
npm run tauri android init
npm run tauri android build -- --apk --target aarch64
```

Exemplos Rust manuais (em `src-tauri/examples/`, exigem TV na rede / token):

```bash
cargo run --example local-test --manifest-path src-tauri/Cargo.toml
```

## Mapa do repositório

| Caminho | Responsabilidade |
|---|---|
| `index.html` | Layout do controle (todos os cartões e overlays) |
| `src/main.ts` | Toda a lógica do frontend; chama comandos via `invoke` |
| `src/styles.css` | Visual |
| `src-tauri/src/lib.rs` | Bootstrap do Tauri; registro dos comandos (`generate_handler!`) |
| `src-tauri/src/commands.rs` | Comandos expostos ao frontend (ponte fina, sem regra de negócio) |
| `src-tauri/src/discovery.rs` | Descoberta de TVs via SSDP + nome/modelo em `http://<ip>:8000/api/v2/` |
| `src-tauri/src/local.rs` | Conexão direta: pareamento, teclas `KEY_*`, apps, texto, estado em `local.json` |
| `src-tauri/src/smartthings.rs` | Cliente SmartThings: auth, comandos, entradas, modos, estado em `smartthings.json` |
| `src-tauri/src/main.rs` | Binário desktop (chama `lib::run`) |
| `src-tauri/examples/` | Scripts manuais de teste contra TV real (`channel-test`, `launch-test`, `local-test`, `inputs-test`) |
| `src-tauri/capabilities/default.json` | Permissões do Tauri |
| `src-tauri/tauri.conf.json` | `productName: Samsung Remote`, `identifier: com.samsungremote.desktop`, janela 1180×820 |

Estado persistente (pasta de dados do app no SO, **fora do repo**): `local.json`
(host, token de pareamento, apps, favoritos) e `smartthings.json` (token, device, capacidades).

## Convenções

- **Frontend**: TypeScript `strict` (`noUnusedLocals`, `noUnusedParameters`). `npx tsc --noEmit`
  precisa passar. Sem framework — DOM + `invoke("@tauri-apps/api/core")`.
- **Teclas da TV**: sempre o vocabulário `KEY_*` (`KEY_POWER`, `KEY_UP`, `KEY_GUIDE`…).
  Novas teclas passam por `local::send_key` (rede) e/ou `smartthings::execute` (nuvem).
- **Nuvem com capability-gating**: botões refletem as capacidades que a TV expõe;
  tecla não suportada aparece apagada, nunca removida. Mapeamento capacidade→componente
  prefere `main` (ver `st_select_device` em `commands.rs`).
- **Prioridade de transporte**: rede local quando conectada; nuvem como reserva e para
  `Power on`. Exceção documentada: troca de canal prioriza a nuvem (`setTvChannel`),
  porque dígitos via rede são consumidos pelo app em primeiro plano na TV.
- **TLS**: `reqwest`/`tokio-rustls` com **rustls** (Rust puro). Não reintroduzir
  `native-tls`/OpenSSL — quebraria o build Android.
- **IDs de apps variam por região/ano**. Preferir a lista real da TV
  (`ed.installedApp.get` / `local_apps`) ao catálogo fixo em `main.ts`.
- **Textos da UI**: pt-BR, tom direto. Erros de comando retornam `Result<_, String>`
  com mensagem amigável já em pt-BR (o frontend exibe via toast).

## Não fazer

- Não commitar segredos: `src-tauri/android-key/`, `*.jks`, `*.keystore`,
  `key.properties`, `.env`, tokens SmartThings. Estão no `.gitignore` — manter assim.
- Não versionar gerados: `node_modules/`, `dist/`, `src-tauri/target/`,
  `src-tauri/gen/schemas/`, APKs/bundles.
- Não mudar `identifier`, `productName` ou a porta `1420` sem motivo explícito.
- Não adicionar dependência nativa (OpenSSL, etc.) sem validar o build Android.
- Não quebrar a compatibilidade com TVs que expõem poucas capacidades (testar com
  capability-gating: botões apagados, app funcional).

## Fluxo de trabalho

1. Descreva o plano antes de mexer em `local.rs`/`smartthings.rs` (protocolo com a TV).
2. Prefira editar arquivos existentes a criar novos; exemplos descartáveis vão em
   `src-tauri/examples/`, não no código de produção.
3. Após alterar frontend: `npx tsc --noEmit`. Após alterar Rust:
   `cargo check --manifest-path src-tauri/Cargo.toml`.
4. Teste real exige TV Samsung na mesma rede (SSDP não atravessa VLANs) e/ou token
   SmartThings com escopos `r:devices:*` + `x:devices:*`.
5. Commits pequenos, em pt-BR, no formato do histórico
   (ex.: `Painel de apps gerenciável: ...`).
