"use strict";

const fs = require("node:fs/promises");
const path = require("node:path");
const os = require("node:os");
const { S3Client, GetObjectCommand } = require("@aws-sdk/client-s3");
const xifty = require("@xifty/xifty");

const s3 = new S3Client({});

exports.handler = async function handler(event) {
  const request = event ?? {};
  const view = request.view ?? "normalized";

  const assetPath = await resolveAssetPath(request);
  const output = xifty.extract(assetPath, { view });

  return {
    source: buildSourceDescription(request, assetPath),
    xifty: output,
  };
};

async function resolveAssetPath(request) {
  if (request.assetPath) {
    return path.resolve(request.assetPath);
  }

  if (request.bucket && request.key) {
    return downloadS3ObjectToTmp(request.bucket, request.key);
  }

  throw new Error(
    "event must include either assetPath or bucket/key so XIFty has a file to inspect",
  );
}

function buildSourceDescription(request, assetPath) {
  if (request.assetPath) {
    return {
      mode: "assetPath",
      assetPath,
    };
  }

  return {
    mode: "s3",
    bucket: request.bucket,
    key: request.key,
    downloadedTo: assetPath,
  };
}

async function downloadS3ObjectToTmp(bucket, key) {
  const response = await s3.send(
    new GetObjectCommand({
      Bucket: bucket,
      Key: key,
    }),
  );

  if (!response.Body || typeof response.Body.transformToByteArray !== "function") {
    throw new Error("S3 response body did not expose transformToByteArray()");
  }

  const bytes = await response.Body.transformToByteArray();
  const fileName = path.basename(key) || "asset.bin";
  const tmpDir = process.env.XIFTY_TMP_DIR || path.join(os.tmpdir(), "xifty");
  await fs.mkdir(tmpDir, { recursive: true });

  const outputPath = path.join(tmpDir, fileName);
  await fs.writeFile(outputPath, Buffer.from(bytes));
  return outputPath;
}
