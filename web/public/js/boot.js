// Corre antes de pintar la página: aplica el tema guardado y avisa a la hoja de estilos que hay
// JavaScript (los controles que lo necesitan quedan ocultos sin él).
(() => {
  const root = document.documentElement;
  root.classList.add("js");
  try {
    const theme = localStorage.getItem("openlight-theme");
    if (theme === "light" || theme === "dark") root.dataset.theme = theme;
  } catch {
    // Sin almacenamiento (ventana privada, datos bloqueados) queda el tema del sistema.
  }
})();
