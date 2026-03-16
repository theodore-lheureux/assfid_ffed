#!/usr/bin/env python3
import argparse
import datetime as dt
import io
import json
import os
import sys
import uuid

import pika

try:
    from PIL import Image
except ImportError:
    Image = None


DEFAULT_URL = "amqp://ffed:password@localhost:5672/%2f"
DEFAULT_STEPS = ["tiff_queue", "indices_queue", "model_data_queue"]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Subscribe to assfid_ffed pipeline exchanges.")
    parser.add_argument("--url", default=DEFAULT_URL, help="AMQP URL")
    parser.add_argument(
        "--step",
        dest="steps",
        action="append",
        help="Pipeline exchange to subscribe to. Repeat for multiple steps.",
    )
    parser.add_argument(
        "--max-preview-bytes",
        type=int,
        default=120,
        help="Max bytes to print from textual payload previews.",
    )
    parser.add_argument(
        "--json-lines",
        action="store_true",
        help="Emit one JSON object per message.",
    )
    parser.add_argument(
        "--no-preview",
        action="store_true",
        help="Do not open TIFF previews for image streams.",
    )
    parser.add_argument(
        "-w",
        "--write",
        action="store_true",
        help="Write each consumed payload to disk.",
    )
    parser.add_argument(
        "--write-dir",
        default="pipeline-dumps",
        help="Directory where payload files are written when -w is enabled.",
    )
    return parser.parse_args()


def write_payload(write_dir: str, stream: str, payload: bytes,
                  content_type: str | None = None) -> str:
    timestamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
    stream_dir = os.path.join(write_dir, stream)
    os.makedirs(stream_dir, exist_ok=True)

    if content_type == "application/json":
        extension = ".json"
    elif content_type == "image/png":
        extension = ".png"
    elif content_type in {"image/tiff", "image/tif"}:
        extension = ".tiff"
    elif stream == "tiff_queue":
        extension = ".tiff"
    elif stream == "model_data_queue":
        extension = ".png"
    elif stream == "indices_queue":
        extension = ".json"
    else:
        extension = ".bin"

    file_name = f"{timestamp}_{uuid.uuid4().hex[:8]}{extension}"
    path = os.path.join(stream_dir, file_name)
    with open(path, "wb") as handle:
        handle.write(payload)
    return path


def make_preview(payload: bytes, max_bytes: int) -> str:
    try:
        text = payload.decode("utf-8")
    except UnicodeDecodeError:
        return "<binary>"

    clean = " ".join(text.split())
    if len(clean) > max_bytes:
        return clean[:max_bytes] + "..."
    return clean


def preview_tiff(stream: str, payload: bytes) -> str:
    if Image is None:
        return "unavailable:pillow-not-installed"

    try:
        with Image.open(io.BytesIO(payload)) as image:
            preview = image.copy()
            if preview.mode not in {"RGB", "RGBA", "L"}:
                preview = preview.convert("RGB")
            preview.thumbnail((1600, 1600))
            preview.show(title=f"{stream} preview")
        return "shown"
    except Exception as exc:
        return f"failed:{exc}"


def main() -> int:
    args = parse_args()
    steps = args.steps if args.steps else DEFAULT_STEPS

    try:
        params = pika.URLParameters(args.url)
        connection = pika.BlockingConnection(params)
        channel = connection.channel()
    except Exception as exc:
        print(f"Failed to connect to RabbitMQ: {exc}", file=sys.stderr)
        return 1

    session = uuid.uuid4().hex[:8]
    print(f"Connected: {args.url}")
    print(f"Session: {session}")
    print(f"Steps: {', '.join(steps)}")

    for step in steps:
        channel.exchange_declare(exchange=step, exchange_type="fanout", durable=True)
        queue_name = f"debug.{session}.{step}"
        channel.queue_declare(queue=queue_name, durable=False, exclusive=False, auto_delete=True)
        channel.queue_bind(queue=queue_name, exchange=step, routing_key="")

        def on_message(ch, method, properties, body, stream=step):
            timestamp = dt.datetime.now(dt.timezone.utc).isoformat()
            preview = make_preview(body, args.max_preview_bytes)
            image_preview = None
            output_path = None
            ct = getattr(properties, "content_type", None)
            if not args.no_preview and ct != "application/json" and stream in {"tiff_queue", "model_data_queue"}:
                image_preview = preview_tiff(stream, body)
            if args.write:
                output_path = write_payload(
                    args.write_dir, stream, body,
                    content_type=getattr(properties, "content_type", None),
                )
            if args.json_lines:
                record = {
                    "ts": timestamp,
                    "stream": stream,
                    "bytes": len(body),
                    "content_type": getattr(properties, "content_type", None),
                    "delivery_mode": getattr(properties, "delivery_mode", None),
                    "preview": preview,
                    "image_preview": image_preview,
                    "written_to": output_path,
                }
                print(json.dumps(record, ensure_ascii=True))
            else:
                print(
                    f"[{timestamp}] stream={stream} bytes={len(body)} "
                    f"content_type={getattr(properties, 'content_type', None)} "
                    f"image_preview={image_preview} written_to={output_path} preview={preview}"
                )
            ch.basic_ack(delivery_tag=method.delivery_tag)

        channel.basic_consume(queue=queue_name, on_message_callback=on_message, auto_ack=False)
        print(f"Subscribed to {step} via queue {queue_name}")

    print("Waiting for messages. Press Ctrl+C to stop.")
    try:
        channel.start_consuming()
    except KeyboardInterrupt:
        print("Stopping...")
    finally:
        try:
            if channel.is_open:
                channel.close()
        finally:
            if connection.is_open:
                connection.close()

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
