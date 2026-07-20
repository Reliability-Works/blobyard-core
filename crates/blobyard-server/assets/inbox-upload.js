(() => {
  "use strict";

  const root = document.querySelector("[data-inbox]");
  const form = root?.querySelector("form");
  const input = root?.querySelector("input[type=file]");
  const button = root?.querySelector("button");
  const message = root?.querySelector("[data-status]");
  if (!(root && form && input && button && message)) return;

  const token = decodeURIComponent(location.pathname.split("/").pop() || "");
  const apiHeaders = () => ({
    accept: "application/json",
    "content-type": "application/json",
    "x-blobyard-inbox-token": token,
  });
  const show = (text, failed = false) => {
    message.textContent = text;
    message.toggleAttribute("data-error", failed);
  };
  const data = async (response) => {
    const envelope = await response.json();
    if (!response.ok || envelope?.ok !== true || !envelope.data) {
      throw new Error(envelope?.error?.message || "The upload could not be completed.");
    }
    return envelope.data;
  };
  const post = async (path, payload, idempotency) => {
    const headers = apiHeaders();
    if (idempotency) headers["idempotency-key"] = idempotency;
    return data(await fetch(path, { method: "POST", headers, body: JSON.stringify(payload) }));
  };
  const localUrl = (value) => {
    const url = new URL(value, location.origin);
    if (url.origin !== location.origin) throw new Error("Blob Yard returned an unsafe upload URL.");
    return url;
  };
  const digest = async (file) => {
    const bytes = new Uint8Array(await crypto.subtle.digest("SHA-256", await file.arrayBuffer()));
    return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  };
  const put = async (url, body, headers = {}) => {
    const response = await fetch(localUrl(url), { method: "PUT", headers, body });
    if (!response.ok) throw new Error("The storage upload failed.");
    return response;
  };
  const single = async (file, grant) => {
    if (typeof grant.uploadUrl !== "string")
      throw new Error("Blob Yard returned an invalid upload URL.");
    await put(
      grant.uploadUrl,
      file,
      Object.fromEntries((grant.headers || []).map(({ name, value }) => [name, value])),
    );
    return [];
  };
  const multipart = async (file, grant) => {
    if (!Number.isSafeInteger(grant.partSizeBytes) || grant.partSizeBytes < 1) {
      throw new Error("Blob Yard returned an invalid part size.");
    }
    const count = Math.ceil(file.size / grant.partSizeBytes);
    const requested = Array.from({ length: count }, (_, index) => index + 1);
    const response = await post("/v1/uploads/parts/request", {
      uploadId: grant.uploadId,
      partNumbers: requested,
    });
    if (!Array.isArray(response.parts) || response.parts.length !== count) {
      throw new Error("Blob Yard returned invalid part URLs.");
    }
    const completed = [];
    for (const part of response.parts) {
      const start = (part.partNumber - 1) * grant.partSizeBytes;
      const upload = await put(
        part.uploadUrl,
        file.slice(start, Math.min(start + grant.partSizeBytes, file.size)),
      );
      const etag = upload.headers.get("etag");
      if (!etag) throw new Error("The storage response did not include an ETag.");
      completed.push({ partNumber: part.partNumber, etag });
    }
    return completed;
  };
  const reserve = async (file) =>
    post(
      "/v1/uploads/request",
      {
        workspace: "inbox",
        project: "inbox",
        path: file.name,
        filename: file.name,
        sizeBytes: file.size,
        checksumSha256: await digest(file),
        contentType: file.type || "application/octet-stream",
      },
      `browser:${crypto.randomUUID()}`,
    );
  const abort = async (uploadId) => {
    try {
      await post("/v1/uploads/abort", { uploadId });
    } catch {
      // The first failure remains the useful result.
    }
  };

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const file = input.files?.item(0);
    if (!file) return show("Choose a file to upload.", true);
    if (file.size > Number(root.dataset.maxBytes)) {
      return show("This file exceeds the remaining inbox byte limit.", true);
    }
    button.disabled = true;
    show("Uploading securely…");
    let grant;
    try {
      grant = await reserve(file);
      const parts =
        grant.strategy === "single" ? await single(file, grant) : await multipart(file, grant);
      const result = await post("/v1/uploads/complete", { uploadId: grant.uploadId, parts });
      show(`${file.name} was uploaded. ${result.uri}`);
      form.reset();
    } catch (error) {
      if (grant?.uploadId) await abort(grant.uploadId);
      show(
        error instanceof Error ? error.message : "The upload could not be completed safely.",
        true,
      );
    } finally {
      button.disabled = false;
    }
  });
})();
