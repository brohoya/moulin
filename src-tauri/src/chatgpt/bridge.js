// Runs a function of Moulin in the chatgpt.com page and keeps its result (see bridge.rs).
// eval gives no result for a promise, so the page keeps the result under a random property until Moulin reads it.
({
  // Starts the code, and keeps its result as JSON text in window[key][id].
  start(key, id, run) {
    const box = window[key] || (window[key] = {});
    box[id] = null;
    Promise.resolve().then(run).then(
      (value) => { box[id] = JSON.stringify({ value: value ?? null }); },
      (error) => { box[id] = JSON.stringify({ error: String((error && error.message) || error) }); },
    );
  },

  // Gives the result and deletes it. The result is null while the code runs, and false if a navigation removed it.
  poll(key, id) {
    const box = window[key];
    if (!box || !(id in box)) return false;
    const value = box[id];
    if (value !== null) delete box[id];
    return value;
  },
})
