# Mi Tienda POS — Desktop Wrapper

Tauri 2 wrapper para [Mi Tienda POS](https://mitiendapos.com.ar). Carga la app web en una WebView nativa con:

- **Sidecar embebido**: agente Go ([`mi-tienda-agente`](https://github.com/Juanmabinda/mi-tienda-agente)) que habla impresoras térmicas USB/LAN/Bluetooth + fiscales.
- **Pareo automático**: detecta sesión owner/admin, intercambia un grant one-time con el server por el `agent_token`. El usuario no ve códigos.
- **Auto-update**: chequea `mitiendapos.com.ar/desktop/manifest.json` al boot, instala silencioso, aplica al próximo restart.
- **Modo kiosko**: opcional, pantalla completa sin decoraciones para POS dedicado.
- **Save dialog nativo**: bridge para descargar XLSX/CSV de reportes (WKWebView no soporta `<a download>`).

## Estructura

```
mi-tienda-desktop/
├── package.json                       # solo @tauri-apps/cli
├── src/                               # frontend mínimo (la URL real es mitiendapos.com.ar)
│   ├── index.html
│   ├── main.js
│   └── styles.css
└── src-tauri/
    ├── Cargo.toml
    ├── build.rs
    ├── tauri.conf.json                # config principal
    ├── Info.plist                     # locale es para WKWebView
    ├── src/
    │   ├── main.rs                    # entry point
    │   └── lib.rs                     # comandos pair_agent, save_file_bytes, kiosk_mode
    ├── capabilities/default.json      # permisos: solo nuestros comandos invocables
    ├── macos/
    │   ├── entitlements.plist         # USB + Bluetooth + network
    │   └── dmg-background.png         # ⚠️ falta — diseño DMG
    ├── windows/
    │   └── installer-hooks.nsi        # shortcut desktop + autostart
    ├── icons/                         # ⚠️ generar con `npx @tauri-apps/cli icon source-logo.png`
    └── binaries/                      # sidecar Go (compilado por workflow)
```

## Setup local (development)

```bash
# Instalar Tauri CLI + deps
npm install

# Compilar el agent localmente y dropearlo en binaries/ con el target triple
# del host. Ejemplo Mac arm64:
cd /path/to/mi-tienda-agente
go build -o /path/to/mi-tienda-desktop/src-tauri/binaries/mi-tienda-print-aarch64-apple-darwin .

# Generar iconos desde el logo fuente (una sola vez)
npx @tauri-apps/cli icon src-tauri/icons/source-logo.png

# Generar Tauri Updater keypair (una sola vez, ver doc/desktop_setup.md en pos)
CI=true npx @tauri-apps/cli signer generate -p '' -w ~/.tauri/mi-tienda-desktop
# Pegar la pubkey en src-tauri/tauri.conf.json → plugins.updater.pubkey

# Run en dev
npx @tauri-apps/cli dev
```

## Release

Bump version en 3 archivos al mismo tiempo:
- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`

Después:
```bash
git tag v0.1.0 && git push origin v0.1.0
```

El workflow `.github/workflows/release.yml` corre matrix Mac+Win, buildea, firma (si los secrets están), publica un Release **draft** en GitHub. Revisás y publicás.

## Documentación completa

Ver `doc/desktop_setup.md` en el repo `pos` para:
- Inventario completo de secrets (Tauri keypair, Apple, SignPath)
- Cómo cargar GH secrets
- Plan de contingencia si se pierde alguno
- Endpoints de Rails que el wrapper consume
