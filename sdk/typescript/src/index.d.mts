import type {
  OperationBindings,
  OperationId,
  OperationInputs,
  OperationOutputs,
  OptionalOperationId,
  RequiredOperationId,
} from "./operations.generated.mjs";

export type JsonPrimitive = boolean | null | number | string;
export type JsonValue =
  JsonPrimitive | readonly JsonValue[] | { readonly [key: string]: JsonValue };
export interface BlobYardClientOptions {
  readonly accessToken?: string | (() => Promise<string | undefined> | string | undefined);
  readonly baseUrl?: string;
  readonly deployment?: "cloud" | "self-hosted";
  readonly fetch?: typeof globalThis.fetch;
}

export interface BlobYardApiErrorOptions {
  readonly code: string;
  readonly message: string;
  readonly requestId: string | null;
  readonly status: number | null;
}

export class BlobYardApiError extends Error {
  readonly code: string;
  readonly requestId: string | null;
  readonly status: number | null;
  constructor(options: BlobYardApiErrorOptions);
}

export class BlobYardClient {
  readonly operations: OperationBindings;
  constructor(options?: BlobYardClientOptions);
  request<Id extends RequiredOperationId>(
    operationId: Id,
    options: OperationInputs[Id],
  ): Promise<OperationOutputs[Id]>;
  request<Id extends OptionalOperationId>(
    operationId: Id,
    options?: OperationInputs[Id],
  ): Promise<OperationOutputs[Id]>;
}

export { operations } from "./operations.generated.mjs";
export type {
  OperationBindings,
  OperationId,
  OperationInputs,
  OperationOutputs,
  OptionalOperationId,
  RequiredOperationId,
  Schemas,
} from "./operations.generated.mjs";
