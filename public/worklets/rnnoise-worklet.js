/**
 * RNNoise AudioWorklet Processor
 * Loads Mozilla's RNNoise WASM binary and processes audio in 480-sample frames
 * (10ms at 48kHz) for broadcast-quality neural noise suppression.
 *
 * The WASM binary must be sent via MessagePort after construction.
 */

class RNNoiseProcessor extends AudioWorkletProcessor {
  constructor() {
    super();

    this.ready = false;
    this.wasmInstance = null;
    this.denoiseState = null;
    this.heapF32 = null;
    this.inputPtr = 0;
    this.outputPtr = 0;

    // RNNoise works on 480-sample frames (10ms at 48kHz)
    this.frameSize = 480;
    this.inputBuffer = new Float32Array(this.frameSize);
    this.outputBuffer = new Float32Array(this.frameSize);
    this.bufferIndex = 0;
    this.outputIndex = 0;
    this.outputReady = false;

    this.enabled = true;
    this.vadProbability = 0;

    this.port.onmessage = async (event) => {
      const data = event.data;

      if (data.type === 'load-wasm') {
        try {
          await this.loadWasm(data.wasmBytes);
          this.port.postMessage({ type: 'ready' });
        } catch (err) {
          this.port.postMessage({ type: 'error', message: err.message });
        }
      }

      if (data.type === 'config') {
        if (data.enabled !== undefined) this.enabled = data.enabled;
      }
    };
  }

  async loadWasm(wasmBytes) {
    const memory = new WebAssembly.Memory({ initial: 256, maximum: 256 });

    const importObject = {
      env: {
        memory,
        // RNNoise WASM imports (math functions it expects)
        expf: Math.exp,
        powf: Math.pow,
        logf: Math.log,
        sinf: Math.sin,
        cosf: Math.cos,
        sqrtf: Math.sqrt,
        floorf: Math.floor,
        ceilf: Math.ceil,
        fabsf: Math.abs,
        tanhf: Math.tanh,
        // Memory management stubs (rnnoise uses static allocation)
        abort: () => { throw new Error('WASM abort'); },
      },
    };

    try {
      const result = await WebAssembly.instantiate(wasmBytes, importObject);
      this.wasmInstance = result.instance;

      const exports = this.wasmInstance.exports;

      // Initialize RNNoise
      if (exports.rnnoise_create) {
        this.denoiseState = exports.rnnoise_create(0); // NULL = use default model
      }

      // Allocate input/output buffers in WASM memory
      if (exports.malloc) {
        this.inputPtr = exports.malloc(this.frameSize * 4); // float32 = 4 bytes
        this.outputPtr = exports.malloc(this.frameSize * 4);
      }

      this.heapF32 = new Float32Array(
        exports.memory ? exports.memory.buffer : memory.buffer
      );

      this.ready = true;
    } catch (err) {
      // If WASM loading fails, we operate in passthrough mode
      console.warn('RNNoise WASM load failed, using passthrough:', err.message);
      this.ready = false;
    }
  }

  processRNNoiseFrame(inputFrame) {
    if (!this.ready || !this.wasmInstance) {
      return inputFrame;
    }

    const exports = this.wasmInstance.exports;

    // Copy input to WASM memory (RNNoise expects short[-32768, 32767] range)
    const inputOffset = this.inputPtr / 4;
    for (let i = 0; i < this.frameSize; i++) {
      this.heapF32[inputOffset + i] = inputFrame[i] * 32768;
    }

    // Run RNNoise
    if (exports.rnnoise_process_frame) {
      this.vadProbability = exports.rnnoise_process_frame(
        this.denoiseState,
        this.outputPtr,
        this.inputPtr
      );
    }

    // Copy output from WASM memory back to float
    const outputOffset = this.outputPtr / 4;
    const result = new Float32Array(this.frameSize);
    for (let i = 0; i < this.frameSize; i++) {
      result[i] = this.heapF32[outputOffset + i] / 32768;
    }

    return result;
  }

  process(inputs, outputs) {
    const input = inputs[0];
    const output = outputs[0];

    if (!input || !input[0]) return true;

    const inputChannel = input[0];
    const outputChannel = output[0];
    const blockSize = inputChannel.length;

    // Passthrough if disabled or WASM not loaded
    if (!this.enabled || !this.ready) {
      for (let i = 0; i < blockSize; i++) {
        outputChannel[i] = inputChannel[i];
      }
      return true;
    }

    // Process samples through 480-sample frame buffer
    for (let i = 0; i < blockSize; i++) {
      // Feed input into accumulation buffer
      this.inputBuffer[this.bufferIndex] = inputChannel[i];
      this.bufferIndex++;

      // When we have a full frame, process it
      if (this.bufferIndex >= this.frameSize) {
        this.outputBuffer = this.processRNNoiseFrame(this.inputBuffer);
        this.bufferIndex = 0;
        this.outputIndex = 0;
        this.outputReady = true;

        // Report VAD to main thread periodically
        this.port.postMessage({
          type: 'vad',
          probability: this.vadProbability,
        });
      }

      // Output from the processed buffer
      if (this.outputReady && this.outputIndex < this.frameSize) {
        outputChannel[i] = this.outputBuffer[this.outputIndex];
        this.outputIndex++;
      } else {
        // Before first full frame is processed, pass through
        outputChannel[i] = inputChannel[i];
      }
    }

    return true;
  }
}

registerProcessor('rnnoise-processor', RNNoiseProcessor);
