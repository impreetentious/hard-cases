// The entry point. Everything else is in App.

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import "./style.css";

const room = document.getElementById("room");
if (!room) {
  throw new Error("the page has no room to mount in");
}
createRoot(room).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
