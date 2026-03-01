(function () {
  var defined = /\/(en|zh)\//;
  var match = window.location.pathname.match(defined);
  if (!match) return;

  var current = match[1];
  var other = current === "en" ? "zh" : "en";
  var label = current === "en" ? "中文" : "EN";

  var link = document.createElement("a");
  link.href = window.location.pathname.replace(
    "/" + current + "/",
    "/" + other + "/"
  );
  link.className = "lang-toggle";
  link.title = current === "en" ? "切换到中文" : "Switch to English";
  link.textContent = label;

  var buttons = document.querySelector(".right-buttons");
  if (buttons) {
    buttons.insertBefore(link, buttons.firstChild);
  }
})();
