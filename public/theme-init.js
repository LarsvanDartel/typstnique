// Applied before first paint (parser-blocking, in <head>) so the saved theme
// shows immediately with no flash of the default.
try {
  var t = localStorage.getItem("theme")
    ?? (matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark");
  document.documentElement.setAttribute("data-theme", t);
} catch (e) {}
