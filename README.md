# Cosmic Tailscale

Applet para el panel COSMIC que controla una instalación **existente** de Tailscale: estado y dispositivos, conectar/desconectar, cambiar cuenta, seleccionar nodo de salida (incluidas regiones Mullvad cuando están disponibles), copiar IP, abrir la URL de autenticación y enviar/recibir archivos con Taildrop. No instala ni inicia `tailscaled`.

## Requisitos

- COSMIC sobre Wayland y Tailscale instalado y en ejecución en el anfitrión.
- Permisos para manejar el daemon: `tailscale status --json` debe funcionar para el usuario; para perfiles y cambios de conexión puede ser necesario configurar un operador de Tailscale (`sudo tailscale set --operator="$USER"`) según la política del equipo. Esta orden modifica los permisos del daemon: aplícala solo si administras el equipo.
- Para recibir con Taildrop, una carpeta de Descargas definida por XDG (`xdg-user-dir DOWNLOAD`) o `$HOME/Downloads`.

El applet muestra errores si el daemon falta, el usuario no tiene permiso o una operación falla. Sin autenticación, el botón Conectar inicia Tailscale y después muestra el enlace de autorización que proporciona el daemon; esperar a completar el inicio de sesión no se considera un fallo. La lista de dispositivos y Taildrop requieren una sesión Tailscale activa y dispositivos compatibles. Los archivos recibidos quedan en Descargas; ante un nombre ocupado, se añade un sufijo numérico en vez de sobrescribirlo. La recepción funciona solo mientras el applet se ejecuta en el panel. Las transferencias interrumpidas permanecen en `Descargas/.cosmic-tailscale-incoming/active-*` y **no** se publican como archivos terminados: revísalas antes de eliminarlas. Los lotes completados `ready-*` sí se recuperan tras reiniciar el applet.

Si aparece **«Acceso a perfiles denegado»**, `tailscale status --json` todavía puede funcionar: consultar el estado no implica tener permiso para listar cuentas o modificar la conexión. El applet mantiene visibles el estado y los controles que sí están disponibles; no cambia permisos por su cuenta. Un administrador puede ejecutar en el anfitrión `sudo tailscale set --operator="$(id -un)"`, y después pulsar **Actualizar**. El icono del panel y el icono de la aplicación usan el SVG de Tailscale proporcionado para este proyecto; el del panel se integra como icono simbólico en el binario y no depende del tema de iconos. El popup utiliza tarjetas y márgenes adaptados al estilo COSMIC.

## Instalación nativa

Con Rust, Cargo, `just` y las dependencias de desarrollo de libcosmic para tu distribución:

```sh
just build-release
just install
```

El destino predeterminado es `$HOME/.local`; configura `rootdir` y `prefix` para instalar en otro lugar. En COSMIC, añade «Cosmic Tailscale» desde el selector de applets del panel. `just uninstall` elimina solo los archivos instalados por la receta. Para Fedora, `just dist` crea un tarball con dependencias vendorizadas y `rpmbuild -ba packaging/cosmic-tailscale.spec` produce un RPM que declara la dependencia de `tailscale`. La compilación RPM usa fuentes vendorizadas sin acceso a la red.

## Flatpak

El manifiesto `packaging/io.github.chispes.CosmicTailscale.json` fija las fuentes Rust y empaqueta únicamente el cliente `tailscale`; utiliza el daemon del anfitrión mediante `/run/tailscale/tailscaled.sock`. Permisos: Wayland, IPC, `/run/tailscale` y Descargas. Los selectores de archivos y las notificaciones usan portales. Es necesario tener instalado `com.system76.Cosmic.BaseApp//stable`, el runtime/SDK Freedesktop 25.08 y la extensión Rust estable de ese SDK.

```sh
flatpak-builder --user --install --force-clean build-dir packaging/io.github.chispes.CosmicTailscale.json
flatpak run io.github.chispes.CosmicTailscale
```

Si el daemon usa un socket distinto, el manifiesto requiere una adaptación explícita; el Flatpak no administra el servicio de sistema. La compilación local sobre una instalación Flatpak sin capacidad de restaurar etiquetas SELinux de BaseApp puede fallar en `flatpak build-init` con `lsetxattr(security.selinux): Operation not supported`; comprueba el host de compilación antes de distribuir el resultado. El manifiesto obtiene el código fuente de una revisión fijada del repositorio público.

**Todavía no está publicado en COSMIC Store.** El [repositorio COSMIC Flatpak](https://github.com/pop-os/cosmic-flatpak) acepta applets que no encajan en Flathub. Su [plantilla de PR](https://github.com/pop-os/cosmic-flatpak/blob/master/.github/PULL_REQUEST_TEMPLATE.md) exige declarar el código generado con IA en los mensajes de commit; advierte que una contribución parcial o totalmente redactada con IA puede cerrarse sin comentarios, y exige que quien la presente comprenda todos los cambios, pueda responder a la revisión y certifique el Developer Certificate of Origin. Este proyecto ha usado asistencia de IA. Antes de solicitar inclusión, una persona responsable debe revisar el código y verificar el Flatpak completo. Flathub tiene una [política diferente](https://docs.flathub.org/docs/for-app-authors/requirements#generative-ai-policy), que no determina la admisión en COSMIC Store.

## Desarrollo

`cargo test --locked` ejecuta las pruebas del CLI y del receptor; `just check` ejecuta Clippy. `just run` inicia el applet nativo. Las traducciones Fluent están en `i18n/en` e `i18n/es`. Licencia MIT; el binario de Tailscale incluido en el Flatpak conserva su propia licencia en `/app/share/licenses/tailscale/`.
