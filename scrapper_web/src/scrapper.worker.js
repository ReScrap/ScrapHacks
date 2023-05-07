import wasm, { MultiPack } from "scrapper";

async function initialize() {
  await wasm();
  let pack;
  let handlers = {
    parse(data) {
      pack = new MultiPack(data);
      return pack.tree();
    },
    download(data) {
      if (pack) {
        let { label, file_index, offset, size } = data;
        return [label, pack.download(file_index, offset, size)];
      }
    },
  };
  self.onmessage = (event) => {
    for (var [name, func] of Object.entries(handlers)) {
      let data = event.data[name];
      if (data) {
        postMessage(Object.fromEntries([[name, func(data)]]));
      }
    }
  };
}

initialize();
