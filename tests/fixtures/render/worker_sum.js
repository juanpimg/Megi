onmessage = function (event) {
  postMessage(event.data.a + event.data.b);
};
