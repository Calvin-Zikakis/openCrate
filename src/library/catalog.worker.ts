/// <reference lib="webworker" />
import { createCatalog } from "./catalog";
import type { WorkerReply, WorkerRequest } from "./types";

const catalog = createCatalog();
self.onmessage = ({ data }: MessageEvent<WorkerRequest>) => {
  let response: WorkerReply;
  try {
    response = "ids" in data
      ? { id: data.id, ids: catalog.ids(data.query) }
      : { id: data.id, page: catalog.read(data.query, data.start, data.count) };
  } catch (error) {
    response = { id: data.id, error: error instanceof Error ? error.message : "The demo catalog could not be read." };
  }
  self.postMessage(response);
};
