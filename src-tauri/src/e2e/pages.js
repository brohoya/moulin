// What the E2E test reads in the pages of Moulin (see observer.rs). Each function gives a value that JSON can hold.
({
  // The pop-up: the state of the pill, its text, the time of the recording, and the hover.
  // The time grows only with the level events of the recorder, so it also proves that they arrive.
  pill() {
    const pill = document.getElementById('pill');
    return {
      state: pill.dataset.state,
      text: document.getElementById('text').textContent,
      time: document.getElementById('time').textContent,
      hover: pill.hasAttribute('data-hover'),
    };
  },

  // The pop-up: true while the pill is red under the pointer.
  hover() {
    return document.getElementById('pill').hasAttribute('data-hover');
  },

  // The pop-up: the text of the pill, and true if the pill shows all of it.
  text() {
    const t = document.getElementById('text');
    return { text: t.textContent, full: t.scrollWidth <= t.clientWidth };
  },

  // The settings window: the height of the window and of its content, and the notice.
  settings() {
    const notice = document.getElementById('notice');
    return { inner: innerHeight, content: document.body.getBoundingClientRect().height, notice: notice.hidden ? null : notice.textContent };
  },
})
