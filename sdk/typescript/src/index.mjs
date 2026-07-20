import { bindOperations, operations } from "./operations.generated.mjs";

const DEFAULT_BASE_URL = "https://api.blobyard.com/v1";
const DEPLOYMENTS = new Set(["cloud", "self-hosted"]);

function isLoopback(hostname) {
  return hostname === "localhost" || hostname === "127.0.0.1" || hostname === "[::1]";
}

function apiBaseUrl(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new TypeError("Blob Yard API URL must be HTTPS, or HTTP on a loopback development host.");
  }
  const validScheme =
    url.protocol === "https:" || (url.protocol === "http:" && isLoopback(url.hostname));
  const validAuthority = url.username === "" && url.password === "";
  const validTail =
    ["", "/", "/v1", "/v1/"].includes(url.pathname) && url.search === "" && url.hash === "";
  if (!validScheme || !validAuthority || !validTail) {
    throw new TypeError("Blob Yard API URL must be HTTPS, or HTTP on a loopback development host.");
  }
  return `${url.origin}/v1`;
}

export class BlobYardApiError extends Error {
  constructor({ code, message, requestId, status }) {
    super(message);
    this.name = "BlobYardApiError";
    this.code = code;
    this.requestId = requestId;
    this.status = status;
  }
}

function queryString(query = {}) {
  const parameters = new URLSearchParams();
  for (const [key, raw] of Object.entries(query)) {
    if (raw === undefined) continue;
    const values = Array.isArray(raw) ? raw : [raw];
    for (const value of values) parameters.append(key, String(value));
  }
  const encoded = parameters.toString();
  return encoded === "" ? "" : `?${encoded}`;
}

function operationPath(template, path = {}) {
  const used = new Set();
  const resolved = template.replace(/\{([^}]+)\}/gu, (_placeholder, name) => {
    const value = path[name];
    if (typeof value !== "string" && typeof value !== "number") {
      throw new TypeError(`Missing Blob Yard path parameter: ${name}`);
    }
    used.add(name);
    return encodeURIComponent(String(value));
  });
  const extra = Object.keys(path).find((name) => !used.has(name));
  if (extra !== undefined) throw new TypeError(`Unexpected Blob Yard path parameter: ${extra}`);
  return resolved;
}

async function tokenValue(token) {
  return typeof token === "function" ? token() : token;
}

function errorRequestId(envelope, response) {
  return typeof envelope?.requestId === "string"
    ? envelope.requestId
    : response.headers.get("x-request-id");
}

function invalidResponse(response, envelope, message) {
  return new BlobYardApiError({
    code: "INVALID_RESPONSE",
    message,
    requestId: errorRequestId(envelope, response),
    status: response.status,
  });
}

async function responseEnvelope(response, operation) {
  let envelope;
  try {
    envelope = await response.json();
  } catch {
    throw invalidResponse(response, undefined, "Blob Yard returned a non-JSON response.");
  }
  if (envelope?.ok === true) {
    if (!response.ok || response.status !== operation.successStatus) {
      throw invalidResponse(response, envelope, "Blob Yard returned an unexpected HTTP status.");
    }
    if ("data" in envelope) return envelope.data;
    throw invalidResponse(response, envelope, "Blob Yard returned an invalid success response.");
  }
  const error = envelope?.error;
  throw new BlobYardApiError({
    code: typeof error?.code === "string" ? error.code : "INVALID_RESPONSE",
    message: typeof error?.message === "string" ? error.message : "Blob Yard rejected the request.",
    requestId: errorRequestId(envelope, response),
    status: response.status,
  });
}

export class BlobYardClient {
  constructor({
    accessToken,
    baseUrl = DEFAULT_BASE_URL,
    deployment = "cloud",
    fetch: fetchImplementation = globalThis.fetch,
  } = {}) {
    if (typeof fetchImplementation !== "function")
      throw new TypeError("A fetch implementation is required.");
    if (!DEPLOYMENTS.has(deployment))
      throw new TypeError("Blob Yard deployment must be cloud or self-hosted.");
    this.accessToken = accessToken;
    this.baseUrl = apiBaseUrl(baseUrl);
    this.deployment = deployment;
    this.fetch = fetchImplementation;
    this.operations = bindOperations(this.request.bind(this));
  }

  async request(operationId, options = {}) {
    const operation = operations[operationId];
    if (operation === undefined) throw new TypeError(`Unknown Blob Yard operation: ${operationId}`);
    if (!operation.deployments.includes(this.deployment)) {
      throw new BlobYardApiError({
        code: "OPERATION_UNSUPPORTED",
        message: "This operation isn't available on the selected Blob Yard deployment.",
        requestId: null,
        status: null,
      });
    }
    const headers = new Headers({ accept: "application/json" });
    if (!operation.public) {
      const token = await tokenValue(this.accessToken);
      if (typeof token === "string" && token !== "")
        headers.set("authorization", `Bearer ${token}`);
    }
    if (options.body !== undefined) headers.set("content-type", "application/json");
    if (operation.idempotency && options.idempotencyKey !== undefined)
      headers.set("idempotency-key", options.idempotencyKey);
    const response = await this.fetch(
      `${this.baseUrl}${operationPath(operation.path, options.path)}${queryString(options.query)}`,
      {
        body: options.body === undefined ? undefined : JSON.stringify(options.body),
        headers,
        method: operation.method,
        signal: options.signal,
      },
    );
    return responseEnvelope(response, operation);
  }
}

export { operations };
