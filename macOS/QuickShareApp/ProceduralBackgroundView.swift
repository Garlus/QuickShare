import SwiftUI
import Metal
import MetalKit

// MARK: - Metal Shader Source (MSL)

private let shaderSource = """
#include <metal_stdlib>
using namespace metal;

// ─── Noise Utilities ─────────────────────────────────────────────

float hash(float2 p) {
    float3 p3 = fract(float3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

float noise(float2 p) {
    float2 i = floor(p);
    float2 f = fract(p);
    float a = hash(i);
    float b = hash(i + float2(1.0, 0.0));
    float c = hash(i + float2(0.0, 1.0));
    float d = hash(i + float2(1.0, 1.0));
    float2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

float fbm(float2 p, int octaves) {
    float value = 0.0;
    float amplitude = 0.5;
    float frequency = 1.0;
    for (int i = 0; i < octaves; i++) {
        value += amplitude * noise(p * frequency);
        frequency *= 2.0;
        amplitude *= 0.5;
    }
    return value;
}

// ─── Signed Distance Field für weiche Blobs ─────────────────────

float sdCircle(float2 p, float2 center, float radius) {
    return length(p - center) - radius;
}

float sdEllipse(float2 p, float2 center, float2 radii) {
    float2 d = p - center;
    float k0 = length(d / radii);
    float k1 = length(d / (radii * radii));
    return k0 * (k0 - 1.0) / k1;
}

// ─── Halftone Dot Pattern ──────────────────────────────────────

float halftone(float2 uv, float scale, float maskStrength) {
    float2 grid = uv * scale;
    float2 cell = fract(grid) - 0.5;
    float dist = length(cell);
    float dotSize = 0.35 * maskStrength;
    return smoothstep(dotSize, dotSize - 0.02, dist);
}

// ─── Vignette ──────────────────────────────────────────────────

float vignette(float2 uv, float strength) {
    float2 center = uv - 0.5;
    return 1.0 - dot(center, center) * strength;
}

// ─── Main Fragment Shader ──────────────────────────────────────

struct VertexOut {
    float4 position [[position]];
    float2 uv;
};

fragment float4 fragmentMain(
    VertexOut in [[stage_in]],
    constant float &time [[buffer(0)]],
    constant float2 &resolution [[buffer(1)]]
) {
    float2 uv = in.uv;
    float2 aspect = float2(resolution.x / resolution.y, 1.0);
    float2 p = uv * aspect;

    // ── 1. Grundfarbe: Fast weiß mit leichtem Verlauf ──────────
    float3 warmWhite = float3(0.97, 0.98, 1.0);
    float3 coolWhite = float3(0.94, 0.97, 1.0);
    float gradientT = uv.y * 0.3 + uv.x * 0.1;
    float3 bgColor = mix(warmWhite, coolWhite, gradientT);

    // ── 2. FBM-Noise für große Farbwolken ──────────────────────
    float slowTime = time * 0.03;

    float cloudNoise1 = fbm(p * 1.5 + float2(slowTime * 0.5, slowTime * 0.3), 4);
    float cloudNoise2 = fbm(p * 2.0 - float2(slowTime * 0.4, slowTime * 0.2), 4);
    float cloudNoise3 = fbm(p * 0.8 + float2(slowTime * 0.2, slowTime * 0.4), 3);

    // ── 3. Farbpalette ─────────────────────────────────────────
    float3 white     = float3(1.0, 1.0, 1.0);
    float3 veryLight = float3(0.933, 0.973, 1.0);
    float3 lightBlue = float3(0.843, 0.945, 1.0);
    float3 cyan      = float3(0.4, 0.851, 1.0);
    float3 blue      = float3(0.184, 0.553, 1.0);

    // ── 4. Weiche Farbwolken (additive Mischung) ───────────────
    float3 col = bgColor;

    float cloudMask1 = smoothstep(0.3, 0.7, cloudNoise1);
    col = mix(col, veryLight, cloudMask1 * 0.4);

    float cloudMask2 = smoothstep(0.35, 0.65, cloudNoise2);
    col = mix(col, lightBlue, cloudMask2 * 0.35);

    float cloudMask3 = smoothstep(0.4, 0.6, cloudNoise3);
    col = mix(col, cyan, cloudMask3 * 0.15);

    // ── 5. Blob-Masken (SDF) ───────────────────────────────────
    float blobTime = time * 0.02;

    float2 b1c = float2(0.25, 0.75) * aspect
               + float2(sin(blobTime) * 0.05, cos(blobTime * 0.7) * 0.03);
    float b1 = sdEllipse(p, b1c, float2(0.35, 0.25) * aspect);
    float b1m = 1.0 - smoothstep(-0.3, 0.5, b1);
    col = mix(col, lightBlue, b1m * 0.25);
    col = mix(col, cyan, b1m * 0.15);

    float2 b2c = float2(0.75, 0.55) * aspect
               + float2(cos(blobTime * 0.8) * 0.04, sin(blobTime * 0.6) * 0.05);
    float b2 = sdEllipse(p, b2c, float2(0.3, 0.2) * aspect);
    float b2m = 1.0 - smoothstep(-0.25, 0.4, b2);
    col = mix(col, veryLight, b2m * 0.2);
    col = mix(col, lightBlue, b2m * 0.1);

    float2 b3c = float2(0.5, 0.2) * aspect
               + float2(sin(blobTime * 0.5) * 0.06, cos(blobTime * 0.9) * 0.04);
    float b3 = sdCircle(p, b3c, 0.18);
    float b3m = 1.0 - smoothstep(-0.2, 0.3, b3);
    col = mix(col, cyan, b3m * 0.2);

    float2 b4c = float2(0.85, 0.85) * aspect
               + float2(cos(blobTime * 0.3) * 0.03, sin(blobTime * 0.4) * 0.02);
    float b4 = sdEllipse(p, b4c, float2(0.2, 0.15) * aspect);
    float b4m = 1.0 - smoothstep(-0.15, 0.25, b4);
    col = mix(col, blue, b4m * 0.08);

    // ── 6. Additive Lichtverläufe ──────────────────────────────
    float diag = dot(normalize(float2(1.0, -1.0)), uv - 0.5) + 0.5;
    col += white * pow(diag, 2.5) * 0.06;

    float rl1 = 1.0 - smoothstep(0.0, 0.6, length(uv - float2(0.35, 0.65)));
    col += veryLight * rl1 * 0.08;

    float rl2 = 1.0 - smoothstep(0.0, 0.5, length(uv - float2(0.7, 0.3)));
    col += lightBlue * rl2 * 0.05;

    // ── 7. Halftone-Dot-Muster ─────────────────────────────────
    float dotMask  = (1.0 - smoothstep(0.0, 0.5, uv.y))
                   * smoothstep(0.0, 0.3, uv.y);
    float dots = halftone(uv, 80.0, dotMask);
    col = mix(col, float3(0.85, 0.92, 0.98), dots * 0.12 * dotMask);

    // ── 8. Subtile Vignette ────────────────────────────────────
    float vig = vignette(uv, 0.3);
    col *= 0.92 + vig * 0.08;

    // ── 9. Taper to white towards toolbar ───────────────────────
    float toolbarFade = pow(uv.y, 3.0);
    col = mix(col, float3(1.0), toolbarFade);

    // ── 10. Feine Rausch-Textur ─────────────────────────────────
    float fineNoise = fbm(p * 8.0 + float2(time * 0.01, time * 0.015), 3);
    col += (fineNoise - 0.5) * 0.015;

    // ── 11. Gamma-Korrektur & Clamping ──────────────────────────
    col = pow(col, float3(1.0 / 1.05));
    col = clamp(col, 0.0, 1.0);

    return float4(col, 1.0);
}

// ─── Vertex Shader ─────────────────────────────────────────────

vertex VertexOut vertexMain(uint vertexID [[vertex_id]]) {
    float2 pos[4] = {
        float2(-1.0, -1.0), float2(1.0, -1.0),
        float2(-1.0,  1.0), float2( 1.0,  1.0)
    };
    float2 uv[4] = {
        float2(0.0, 1.0), float2(1.0, 1.0),
        float2(0.0, 0.0), float2(1.0, 0.0)
    };
    VertexOut out;
    out.position = float4(pos[vertexID], 0.0, 1.0);
    out.uv = uv[vertexID];
    return out;
}
"""

// MARK: - Metal Coordinator

fileprivate class MetalCoordinator: NSObject, MTKViewDelegate {
    let device: MTLDevice
    let commandQueue: MTLCommandQueue
    let pipelineState: MTLRenderPipelineState
    let timeBuffer: MTLBuffer
    let resolutionBuffer: MTLBuffer
    let startTime: Date

    init?(device: MTLDevice) {
        self.device = device
        guard let commandQueue = device.makeCommandQueue() else { return nil }
        self.commandQueue = commandQueue

        let library: MTLLibrary
        do {
            library = try device.makeLibrary(source: shaderSource, options: nil)
        } catch {
            print("Shader compilation error: \(error)")
            return nil
        }

        guard let vertexFunc = library.makeFunction(name: "vertexMain"),
              let fragmentFunc = library.makeFunction(name: "fragmentMain") else {
            return nil
        }

        let desc = MTLRenderPipelineDescriptor()
        desc.vertexFunction = vertexFunc
        desc.fragmentFunction = fragmentFunc
        desc.colorAttachments[0].pixelFormat = .bgra8Unorm

        do {
            pipelineState = try device.makeRenderPipelineState(descriptor: desc)
        } catch {
            print("Pipeline error: \(error)")
            return nil
        }

        timeBuffer = device.makeBuffer(length: MemoryLayout<Float>.size, options: [])!
        resolutionBuffer = device.makeBuffer(length: MemoryLayout<SIMD2<Float>>.size, options: [])!
        startTime = Date()

        super.init()
    }

    func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {
        var res = SIMD2<Float>(Float(size.width), Float(size.height))
        memcpy(resolutionBuffer.contents(), &res, MemoryLayout<SIMD2<Float>>.size)
    }

    func draw(in view: MTKView) {
        guard let drawable = view.currentDrawable,
              let rpd = view.currentRenderPassDescriptor else { return }

        var elapsed = Float(Date().timeIntervalSince(startTime))
        memcpy(timeBuffer.contents(), &elapsed, MemoryLayout<Float>.size)

        guard let cmdBuf = commandQueue.makeCommandBuffer(),
              let enc = cmdBuf.makeRenderCommandEncoder(descriptor: rpd) else { return }

        enc.setRenderPipelineState(pipelineState)
        enc.setFragmentBuffer(timeBuffer, offset: 0, index: 0)
        enc.setFragmentBuffer(resolutionBuffer, offset: 0, index: 1)
        enc.drawPrimitives(type: .triangleStrip, vertexStart: 0, vertexCount: 4)
        enc.endEncoding()

        cmdBuf.present(drawable)
        cmdBuf.commit()
    }
}

// MARK: - SwiftUI Representable

struct ProceduralBackgroundView: NSViewRepresentable {
    func makeNSView(context: Context) -> MTKView {
        guard let device = MTLCreateSystemDefaultDevice() else {
            fatalError("Metal is not supported on this device")
        }

        let view = MTKView()
        view.device = device
        view.colorPixelFormat = .bgra8Unorm
        view.framebufferOnly = false
        view.preferredFramesPerSecond = 30
        view.enableSetNeedsDisplay = false
        view.isPaused = false

        let coordinator = MetalCoordinator(device: device)!
        view.delegate = coordinator
        context.coordinator.storage = coordinator

        coordinator.mtkView(view, drawableSizeWillChange: view.drawableSize)

        return view
    }

    func updateNSView(_ nsView: MTKView, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator() }

    class Coordinator {
        fileprivate var storage: MetalCoordinator?
    }
}
