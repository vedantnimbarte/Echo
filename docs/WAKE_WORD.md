# Wake word

Echo can listen for a spoken phrase and start dictating without you touching
the keyboard. It is **off by default** — the microphone stays closed until you
press your shortcut, unless you turn this on in Settings → Wake word (or in the
last step of onboarding).

## How it works

Detection is [openWakeWord](https://github.com/dscripka/openWakeWord): a
three-stage ONNX chain that runs entirely on your machine.

```
audio (16 kHz)  →  melspectrogram.onnx  →  32-bin mel frames
                →  embedding_model.onnx (76 frames → 96-d vector)
                →  <phrase>.onnx        (16 vectors → score 0..1)
```

Two properties matter:

- **It reuses the ONNX Runtime already linked into Echo** for the Silero VAD, so
  the feature adds no new dependency and nothing extra to ship in the installer.
- **The spotter is gated behind the VAD.** The three-stage chain only runs on
  audio frames the VAD already flagged as speech, so an idle room costs a VAD
  pass rather than a full wake-word inference.

Models are downloaded on first enable rather than committed to the repo. The two
feature models are shared by every phrase, so switching phrases later only
fetches a ~1 MB classifier. They land in `<app data>/wake/`.

## What it costs you

Be honest with users about this, because it is the part that generates support
tickets:

- **Your OS microphone indicator stays lit** the entire time listening is armed
  (macOS especially). This is correct and unavoidable — the microphone genuinely
  is open.
- **macOS** needs continuous microphone permission, and Accessibility permission
  for the text injection that follows.
- **False accepts and false rejects trade off against each other.** The
  sensitivity slider is the control: lower catches the phrase more often and
  misfires more. There is no setting that eliminates both.

## Phrases

Echo ships the pretrained openWakeWord catalog — "Hey Jarvis" (the default),
"Alexa", "Hey Mycroft", "Hey Rhasspy". **None of these is "Hey Echo"**: training
that model is the separate step below, and until it exists "Hey Jarvis" is the
most reliable option.

## Training a custom phrase (including "Hey Echo")

openWakeWord trains from synthetic speech, so you do **not** record yourself.

1. Open upstream's
   [`notebooks/automatic_model_training.ipynb`](https://github.com/dscripka/openWakeWord/blob/main/notebooks/automatic_model_training.ipynb)
   in Colab (it wants a GPU; CPU training is slow but works).
2. Set the target phrase to `hey echo` and run the notebook through to the end.
3. Download the resulting `.onnx` classifier.
4. In Echo: Settings → Wake word → **Import custom**, and pick that file.

The import copies it to `<app data>/wake/custom.onnx` and selects it. The shared
feature models are fetched first if you have not downloaded any phrase yet.

Test it before relying on it: say the phrase ten times in your normal voice, at
your normal distance, with your normal background noise, and count the misses.
Then leave it armed for an hour of ordinary work and count the misfires. Adjust
the sensitivity slider from what those two numbers tell you.

## Bundling a trained "Hey Echo" for everyone

Once a `hey_echo.onnx` exists and tests well, host it on a release and add it to
`PHRASE_CATALOG` in
[`core/wake/mod.rs`](../echo-app/src-tauri/src/core/wake/mod.rs) alongside the
upstream entries, then make it `DEFAULT_PHRASE`. The catalog entry is three
lines; nothing else has to change.

Note the pinned `RELEASE` constant in that file: the feature models and the
phrase classifiers are trained together and must stay in step, so bump it
deliberately.

## Command mode

Wake word pairs with command mode (Settings → Command mode), which is also off
by default. With it on, a transcript that opens with your trigger word is sent
to an LLM instead of being typed:

- **With text selected** in the focused app, the instruction is applied to that
  text and the result replaces it.
- **With nothing selected**, the answer is typed at your cursor.

The backend defaults to a local [Ollama](https://ollama.com) server so selected
text never leaves your machine. Switching it to OpenAI reuses the API key
already in your keychain — and does send the selected text to OpenAI, which the
settings panel warns about.

Reading the selection works by synthesizing the OS copy shortcut and reading the
clipboard, then restoring what was on the clipboard before.
