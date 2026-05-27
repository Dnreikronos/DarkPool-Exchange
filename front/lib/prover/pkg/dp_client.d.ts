export function compute_commitment_wasm(
  commitment_key: string,
  side: number,
  price: string,
  size: string,
  salt_hex: string,
): string;
export function derive_trader_id_wasm(commitment_key: string): string;
export function encrypt_order_wasm(
  pubkey_hex: string,
  order_json: string,
): Uint8Array;
export function prepare_order_wasm(
  operator_pubkey_hex: string,
  order_json: string,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
): any;

export type InitInput =
  | RequestInfo
  | URL
  | Response
  | BufferSource
  | WebAssembly.Module;

export default function init(
  module_or_path?: InitInput | Promise<InitInput>,
): Promise<void>;
