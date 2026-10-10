// Content scripts cannot be WebAssembly, so this loads the Rust module that
// does the actual work (see src/lib.rs).
(async () => {
  const runtime = (globalThis.browser ?? globalThis.chrome).runtime;
  const { default: init } = await import(runtime.getURL("pkg/gmcb_extension.js"));
  await init({ module_or_path: runtime.getURL("pkg/gmcb_extension_bg.wasm") });
})().catch((error) => console.error("GMod Collection Builder failed to start", error));
