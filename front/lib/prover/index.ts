type WasmExports = typeof import('./pkg/dp_client');

let cached: WasmExports | null = null;

async function getModule(): Promise<WasmExports> {
  if (cached) return cached;
  const wasm: WasmExports = await import(
    /* webpackIgnore: true */ './pkg/dp_client'
  );
  await wasm.default();
  cached = wasm;
  return wasm;
}

export async function computeCommitment(
  commitmentKey: string,
  side: number,
  price: string,
  size: string,
  saltHex: string,
): Promise<string> {
  const m = await getModule();
  return m.compute_commitment_wasm(commitmentKey, side, price, size, saltHex);
}

export async function deriveTraderIdWasm(
  commitmentKey: string,
): Promise<string> {
  const m = await getModule();
  return m.derive_trader_id_wasm(commitmentKey);
}

export async function prepareOrderWasm(
  operatorPubkeyHex: string,
  orderJson: string,
): Promise<string> {
  const m = await getModule();
  const raw = m.prepare_order_wasm(operatorPubkeyHex, orderJson);
  if (typeof raw === 'string') return raw;
  return JSON.stringify(raw);
}

export async function encryptOrderWasm(
  pubkeyHex: string,
  orderJson: string,
): Promise<Uint8Array> {
  const m = await getModule();
  return m.encrypt_order_wasm(pubkeyHex, orderJson);
}
