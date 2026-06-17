// Applied before first paint (parser-blocking, in <head>) so the saved theme
// shows immediately with no flash of the default.
try {
  var t = localStorage.getItem("theme");
  if (t) {
    document.documentElement.setAttribute("data-theme", t);
  }
} catch (e) {}
