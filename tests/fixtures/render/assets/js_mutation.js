// External classic script used by the render scripting tests: mutates the DOM.
var externalTarget = document.getElementById("target");
externalTarget.textContent = "EXTERNAL";
externalTarget.style.color = "#0000ff";
