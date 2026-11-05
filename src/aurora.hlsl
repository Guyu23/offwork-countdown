// Port of "Auroras" by nimitz (Shadertoy XtGGRt, 2017), adapted to WPF ps_3_0.
sampler2D input : register(s0);
float time : register(c0);
float2 resolution : register(c1);

#define FOV 1.3
#define AUTO_SPEED 0.05
#define AUTO_AMP 0.2
#define RO_Z -6.7
#define NOISE_SPEED 0.06
#define AURORA_NOISE 2.5
#define AURORA_BRIGHTNESS 3.0
#define STARS_BRIGHTNESS 1.0
#define REFLECTION_FADE 2.5

float2 rot(float2 p, float a)
{
    float c = cos(a);
    float s = sin(a);
    return float2(p.x * c - p.y * s, p.x * s + p.y * c);
}

float tri(float x)
{
    return clamp(abs(frac(x) - 0.5), 0.01, 0.49);
}

float2 tri2(float2 p)
{
    return float2(tri(p.x) + tri(p.y), tri(p.y + tri(p.x)));
}

float triNoise2d(float2 p, float spd)
{
    float z = 4.8 - AURORA_BRIGHTNESS;
    float z2 = AURORA_NOISE;
    float rz = 0.0;
    p = rot(p, p.x * 0.06);
    float2 bp = p;
    for (int i = 0; i < 5; i++)
    {
        float2 dg = tri2(bp * 1.85) * 0.75;
        dg = rot(dg, time * spd);
        p -= dg / z2;
        bp *= 1.3;
        z2 *= 0.45;
        z *= 0.42;
        p *= 1.21 + (rz - 1.0) * 0.02;
        rz += tri(p.x + tri(p.y)) * z;
        p = -rot(p, 0.3);
    }
    return clamp(1.0 / pow(rz * 29.0, 1.3), 0.0, 0.55);
}

float hash21(float2 n)
{
    return frac(sin(dot(n, float2(12.9898, 4.1414))) * 43758.5453);
}

float4 smoothstep4(float a, float b, float4 x)
{
    return float4(
        smoothstep(a, b, x.x),
        smoothstep(a, b, x.y),
        smoothstep(a, b, x.z),
        smoothstep(a, b, x.w));
}

float4 aurora(float3 ro, float3 rd, float dither)
{
    float4 col = float4(0.0, 0.0, 0.0, 0.0);
    float4 avgCol = float4(0.0, 0.0, 0.0, 0.0);

    for (int i = 0; i < 50; i++)
    {
        float fi = (float)i;
        float of = 0.006 * dither * smoothstep(0.0, 15.0, fi);
        float pt = ((0.8 + pow(fi, 1.4) * 0.002) - ro.y) / (rd.y * 2.0 + 0.4);
        pt -= of;
        float3 bpos = ro + pt * rd;
        float2 p = bpos.zx;
        float rzt = triNoise2d(p, NOISE_SPEED);
        float4 col2 = float4(0.0, 0.0, 0.0, rzt);
        col2.rgb = (sin(1.0 - float3(2.15, -0.5, 1.2) + fi * 0.043) * 0.5 + 0.5) * rzt;
        avgCol = lerp(avgCol, col2, 0.5);
        col += avgCol * exp2(-fi * 0.065 - 2.5) * smoothstep(0.0, 5.0, fi);
    }

    col *= clamp(rd.y * 15.0 + 0.4, 0.0, 1.0);
    return col * 1.8;
}

float3 hash33(float3 p)
{
    p = frac(p * float3(443.8975, 397.2973, 491.1871));
    p += dot(p.zxy, p.yxz + 19.27);
    return frac(float3(p.x * p.y, p.z * p.x, p.y * p.z));
}

float3 stars(float3 p)
{
    float3 c = float3(0.0, 0.0, 0.0);
    float res = resolution.x;
    for (int i = 0; i < 4; i++)
    {
        float fi = (float)i;
        float3 q = frac(p * (0.15 * res)) - 0.5;
        float3 id = floor(p * (0.15 * res));
        float2 rn = hash33(id).xy;
        float c2 = 1.0 - smoothstep(0.0, 0.6, length(q));
        c2 *= step(rn.x, 0.0005 + fi * fi * 0.001);
        c += c2 * (lerp(float3(1.0, 0.49, 0.1), float3(0.75, 0.9, 1.0), rn.y) * 0.1 + 0.9);
        p *= 1.3;
    }
    return c * c * 0.8;
}

float3 bg(float3 rd)
{
    float sd = dot(normalize(float3(-0.5, -0.6, 0.9)), rd) * 0.5 + 0.5;
    sd = pow(sd, 5.0);
    float3 col = lerp(float3(0.05, 0.1, 0.2), float3(0.1, 0.05, 0.2), sd);
    return col * 0.63;
}

float4 main(float2 uv : TEXCOORD0) : COLOR0
{
    float2 q = float2(uv.x, 1.0 - uv.y);
    float2 p = q - 0.5;
    p.x *= resolution.x / resolution.y;

    float3 ro = float3(0.0, 0.0, RO_Z);
    float3 rd = normalize(float3(p, FOV));

    float angle = sin(time * AUTO_SPEED) * AUTO_AMP;
    float2 xz = rot(rd.xz, angle);
    rd.xz = xz;

    float3 col = float3(0.0, 0.0, 0.0);
    float dither = hash21(q * resolution);
    float3 brd = rd;
    float fade = smoothstep(0.0, 0.01, abs(brd.y)) * 0.1 + 0.9;
    col = bg(rd) * fade;

    if (rd.y > 0.0)
    {
        float4 aur = smoothstep4(0.0, 1.5, aurora(ro, rd, dither)) * fade;
        col += stars(rd) * STARS_BRIGHTNESS;
        col = col * (1.0 - aur.a) + aur.rgb;
    }
    else
    {
        rd.y = abs(rd.y);
        col = bg(rd) * fade * 0.6;
        float4 aur = smoothstep4(0.0, 5.0 - REFLECTION_FADE, aurora(ro, rd, dither));
        col += stars(rd) * STARS_BRIGHTNESS * 0.3;
        col = col * (1.0 - aur.a) + aur.rgb;
        float3 pos = ro + ((0.5 - ro.y) / rd.y) * rd;
        float nz2 = triNoise2d(pos.xz * float2(0.5, 0.7), 0.0);
        col += lerp(float3(0.2, 0.25, 0.5) * 0.08, float3(0.3, 0.3, 0.5) * 0.7, saturate(nz2 * 0.4));
    }

    col *= 0.64 + 0.36 * smoothstep(0.0, 0.30, 1.0 - q.y);
    return float4(col, 1.0);
}
