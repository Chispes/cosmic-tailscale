# COSMIC Tailscale

Applet para COSMIC que controla el daemon Tailscale del anfitrión: conexión, cuentas, nodos de salida, equipos en línea y Taildrop.

## Requisitos e instalación

Se necesita COSMIC sobre Wayland, `tailscaled` activo y Tailscale instalado en el anfitrión. En Fedora, instala juntos los paquetes RPM `cosmic-tailscale` y `cosmic-tailscale-helper` desde un gestor de paquetes gráfico. El helper instala un servicio D-Bus del sistema, una acción Polkit y el agente gráfico lxpolkit para COSMIC. Si el agente no está activo inmediatamente después de la instalación, cierra y vuelve a iniciar sesión. El applet consulta el estado sin solicitar autorización; si Tailscale deniega una operación, muestra «Autorizar este usuario…» y Polkit solicita la aprobación del administrador. Cancelar no cambia el operador ni reintenta la operación. Tras aprobar, pulsa de nuevo Conectar o la acción deseada.

**La autorización reemplaza al operador anterior de Tailscale y persiste en el anfitrión.** Un administrador puede revocarla con `sudo tailscale set --operator=`. No es necesario introducir ese comando para autorizar al nuevo usuario desde el applet. El servicio no admite comandos ni nombres de usuario proporcionados por el cliente.

Para compilar e instalar solo el applet en `$HOME/.local`:

```sh
just build-release
just install
```

Esta instalación local **no instala el helper privilegiado**. Para crear los RPM Fedora con el servicio del anfitrión incluido: `just dist` y `rpmbuild -ba packaging/cosmic-tailscale.spec` (coloca el tarball generado en el directorio `SOURCES` de RPM). Necesitarás Cargo, Rust, just y las dependencias de desarrollo de libcosmic.

## Flatpak

```sh
flatpak-builder --user --install --force-clean build-dir packaging/io.github.chispes.CosmicTailscale.json
flatpak run io.github.chispes.CosmicTailscale
```

El Flatpak incluye el cliente `tailscale` y usa el socket del daemon del anfitrión. Solo recibe acceso D-Bus al nombre del helper, no privilegios generales: **instala por separado el RPM `cosmic-tailscale-helper` en el anfitrión desde un gestor gráfico de paquetes** para disponer del diálogo de administrador. Instalar únicamente el Flatpak no puede instalar servicios privilegiados ni cambiar el operador. Se necesitan `com.system76.Cosmic.BaseApp//stable`, Freedesktop 25.08 y la extensión Rust estable del SDK. Los selectores de archivos y las notificaciones usan portales.

## Uso

Los equipos en línea aparecen en filas compactas: Detalles muestra IP, DNS y acciones de copia/Taildrop solo para el equipo abierto. Cuentas y nodos de salida se despliegan bajo demanda. El enlace de autorización de Tailscale se abre solo cuando el daemon proporciona una URL HTTPS válida. Los archivos Taildrop recibidos se guardan en Descargas sin sobrescribir archivos existentes.

## Desarrollo

`cargo test --locked` ejecuta las pruebas; `just check` ejecuta Clippy. `just run` inicia el applet nativo. Las traducciones Fluent están en `i18n/en` e `i18n/es`. Licencia MIT; el cliente Tailscale incluido en Flatpak conserva su propia licencia.
