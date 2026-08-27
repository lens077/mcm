import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "../design/tokens.css";
import "../design/global.css";
import "./App.css";

const root = document.getElementById("root");
if (!root) throw new Error("Missing #root element");

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
