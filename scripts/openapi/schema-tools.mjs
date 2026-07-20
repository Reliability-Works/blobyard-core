export function pascal(value) {
  return `${value[0].toUpperCase()}${value.slice(1)}`;
}

export function referenceName(reference, prefix = "#/components/schemas/") {
  if (typeof reference?.$ref !== "string" || !reference.$ref.startsWith(prefix)) {
    throw new Error(`expected a ${prefix} reference`);
  }
  return reference.$ref.slice(prefix.length);
}

function operationResponseSchema(document, operation, name) {
  const successResponses = Object.entries(operation.responses ?? {}).filter(([status]) =>
    /^2\d\d$/u.test(status),
  );
  if (successResponses.length !== 1) {
    throw new Error(`${operation.operationId} must declare exactly one success response`);
  }
  const [status, response] = successResponses[0];
  const schema = response?.content?.["application/json"]?.schema;
  const envelopeName = referenceName(schema);
  if (envelopeName !== `${name}SuccessEnvelope`) {
    throw new Error(`${operation.operationId} must use ${name}SuccessEnvelope for success`);
  }
  const envelope = document.components?.schemas?.[envelopeName];
  const resultName = referenceName(envelope?.properties?.data);
  if (resultName !== `${name}Result`) {
    throw new Error(`${operation.operationId} must use ${name}Result for success data`);
  }
  if (operation.responses?.default?.$ref !== "#/components/responses/ApiError") {
    throw new Error(`${operation.operationId} must use the standard error response`);
  }
  return { resultName, successStatus: Number(status) };
}

function operationRequest(operation, name) {
  const body = operation.requestBody;
  if (body === undefined) return undefined;
  if (body.required !== true)
    throw new Error(`${operation.operationId} request body must be required`);
  const schema = body.content?.["application/json"]?.schema;
  const bodyName = referenceName(schema);
  if (bodyName !== `${name}Request`) {
    throw new Error(`${operation.operationId} must use ${name}Request`);
  }
  return bodyName;
}

function parameterSchema(operation, location, name) {
  const parameters = (operation.parameters ?? []).filter((item) => item.in === location);
  if (parameters.length === 0) return undefined;
  const properties = Object.fromEntries(
    parameters.map((parameter) => [parameter.name, parameter.schema]),
  );
  const required = parameters
    .filter((parameter) => parameter.required === true)
    .map((item) => item.name);
  return {
    schema: { additionalProperties: false, properties, required, type: "object" },
    required: required.length > 0,
    name,
  };
}

function idempotency(operation) {
  const headers = (operation.parameters ?? []).filter((item) => item.in === "header");
  const header = headers.find((item) => item.name.toLowerCase() === "idempotency-key");
  if (headers.length !== (header === undefined ? 0 : 1)) {
    throw new Error(`${operation.operationId} has unsupported header parameters`);
  }
  return {
    present: header !== undefined,
    required: header?.required === true,
  };
}

export function operationContract(document, path, operation) {
  const name = pascal(operation.operationId);
  const response = operationResponseSchema(document, operation, name);
  const placeholders = [...path.matchAll(/\{([^}]+)\}/gu)].map((match) => match[1]);
  const pathInput = parameterSchema(operation, "path", `${name}Path`);
  const actualPathNames = Object.keys(pathInput?.schema.properties ?? {});
  if (
    placeholders.length !== actualPathNames.length ||
    placeholders.some((item) => !actualPathNames.includes(item))
  ) {
    throw new Error(`${operation.operationId} path parameters do not match ${path}`);
  }
  const queryInput = parameterSchema(operation, "query", `${name}Query`);
  const idempotencyInput = idempotency(operation);
  return {
    bodyName: operationRequest(operation, name),
    idempotency: idempotencyInput.present,
    idempotencyRequired: idempotencyInput.required,
    inputRequired:
      pathInput?.required === true ||
      queryInput?.required === true ||
      operation.requestBody !== undefined,
    pathInput,
    queryInput,
    resultName: response.resultName,
    successStatus: response.successStatus,
  };
}

function literal(value) {
  return JSON.stringify(value);
}

export function schemaType(schema) {
  if (schema?.$ref !== undefined) return `Schemas[${literal(referenceName(schema))}]`;
  if (Object.hasOwn(schema ?? {}, "const")) return literal(schema.const);
  if (Array.isArray(schema?.enum)) return schema.enum.map(literal).join(" | ");
  if (Array.isArray(schema?.anyOf)) return schema.anyOf.map(schemaType).join(" | ");
  if (Array.isArray(schema?.oneOf)) return schema.oneOf.map(schemaType).join(" | ");
  if (Array.isArray(schema?.allOf)) return schema.allOf.map(schemaType).join(" & ");
  if (Array.isArray(schema?.type)) {
    return schema.type.map((type) => schemaType({ ...schema, type })).join(" | ");
  }
  if (schema?.type === "string") return "string";
  if (schema?.type === "number" || schema?.type === "integer") return "number";
  if (schema?.type === "boolean") return "boolean";
  if (schema?.type === "null") return "null";
  if (schema?.type === "array") return `readonly (${schemaType(schema.items)})[]`;
  if (schema?.type === "object" || schema?.properties !== undefined) return objectType(schema);
  throw new Error("schema cannot be represented as a TypeScript type");
}

function objectType(schema) {
  const properties = Object.entries(schema.properties ?? {});
  const required = new Set(schema.required ?? []);
  const fields = properties.map(
    ([name, property]) =>
      `readonly ${literal(name)}${required.has(name) ? "" : "?"}: ${schemaType(property)};`,
  );
  if (schema.additionalProperties !== undefined && schema.additionalProperties !== false) {
    if (schema.additionalProperties === true) {
      if (schema["x-blobyard-opaque"] !== true) {
        throw new Error("untyped additional properties require x-blobyard-opaque");
      }
      return "Readonly<Record<string, JsonValue>>";
    }
    if (fields.length === 0)
      return `Readonly<Record<string, ${schemaType(schema.additionalProperties)}>>`;
    fields.push(`readonly [key: string]: ${schemaType(schema.additionalProperties)};`);
  }
  return fields.length === 0
    ? "Readonly<Record<string, never>>"
    : `Readonly<{ ${fields.join(" ")} }>`;
}

export function validateClassifications(document) {
  const progress = document["x-blobyard-classifications"]?.onboardingProgress;
  if (
    progress?.classification !== "ui-state" ||
    progress.sdk !== "excluded" ||
    progress.cli !== "excluded" ||
    progress.mcp !== "excluded"
  ) {
    throw new Error("onboardingProgress must be explicitly classified as excluded UI state");
  }
}
