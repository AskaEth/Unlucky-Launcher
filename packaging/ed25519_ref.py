"""Ed25519 的**参考实现**（RFC 8032），只给发布侧签名脚本用（M190）。

为什么这里自写一套：发布侧签名必须**零第三方依赖**（CI 不装 pip 包），而仓里 `crates/core` 的验签
用的是 `ed25519-dalek`。两套实现互相独立，`sign_manifest.py --self-test` 用 RFC 8032 §7.1 的同一组
向量把两边各自钉住 —— 任何一边坏了，向量那一半会红；两边都「一致地错」的概率极低。

⚠ 这里的私钥路径（`sign`）**只在发布侧**（`packaging/sign_manifest.py --sign`）跑，不进任何产物；
二进制里只有验签（`ed25519-dalek::verify_strict`）。扩展坐标公式与 dalek/ref10 同源，向量自检兜底。

    import ed25519_ref as ref
    seed = bytes.fromhex("…")            # 32 字节种子
    pk = ref.public_key(seed)
    sig = ref.sign(seed, b"message")     # 64 字节
    assert ref.verify(pk, b"message", sig)
"""

from __future__ import annotations

import hashlib

ALG = "ed25519"

Q = 2**255 - 19
L = 2**252 + 27742317777372353535851937790883648493

def _inv(x: int) -> int:
    return pow(x, Q - 2, Q)

D = -121665 * _inv(121666) % Q
_D2 = 2 * D % Q
_SQRT_M1 = pow(2, (Q - 1) // 4, Q)

def _recover_x(y: int, sign: int) -> int | None:
    u = (y * y - 1) % Q
    v = (D * y * y + 1) % Q
    v3 = pow(v, 3, Q)
    v7 = pow(v, 7, Q)
    x = u * v3 % Q * pow(u * v7 % Q, (Q - 5) // 8, Q) % Q
    if (v * x * x - u) % Q != 0:
        x = x * _SQRT_M1 % Q
        if (v * x * x - u) % Q != 0:
            return None
    if x % 2 != sign:
        x = (Q - x) % Q
    return x

def _decompress(data: bytes) -> tuple[int, int, int, int] | None:
    if len(data) != 32:
        return None
    y = int.from_bytes(data, "little")
    sign = (y >> 255) & 1
    y &= (1 << 255) - 1
    if y >= Q:
        return None
    x = _recover_x(y, sign)
    if x is None:
        return None
    return (x, y, 1, x * y % Q)

def _add(p: tuple[int, int, int, int], r: tuple[int, int, int, int]):
    x1, y1, z1, t1 = p
    x2, y2, z2, t2 = r
    a = (y1 - x1) * (y2 - x2) % Q
    b = (y1 + x1) * (y2 + x2) % Q
    c = t1 * _D2 * t2 % Q
    d = 2 * z1 * z2 % Q
    e, f, g, h = (b - a) % Q, (d - c) % Q, (d + c) % Q, (b + a) % Q
    return (e * f % Q, g * h % Q, f * g % Q, e * h % Q)

def _double(p: tuple[int, int, int, int]):
    x1, y1, z1, _ = p
    a = x1 * x1 % Q
    b = y1 * y1 % Q
    c = 2 * z1 * z1 % Q
    d = -a % Q
    e = ((x1 + y1) * (x1 + y1) - a - b) % Q
    g = (d + b) % Q
    f = (g - c) % Q
    h = (d - b) % Q
    return (e * f % Q, g * h % Q, f * g % Q, e * h % Q)

def _scalarmult(p, e: int):
    acc = (0, 1, 1, 0)
    for bit in bin(e)[2:] if e else "":
        acc = _double(acc)
        if bit == "1":
            acc = _add(acc, p)
    return acc

def _encode(p) -> bytes:
    x, y, z, _ = p
    zi = _inv(z)
    x, y = x * zi % Q, y * zi % Q
    return (y | ((x & 1) << 255)).to_bytes(32, "little")

def _base():
    encoded = (4 * _inv(5) % Q).to_bytes(32, "little")
    return _decompress(encoded)

def _hint(data: bytes) -> int:
    return int.from_bytes(hashlib.sha512(data).digest(), "little") % L

def public_key(seed: bytes) -> bytes:
    h = hashlib.sha512(seed).digest()
    a = int.from_bytes(_clamp(bytearray(h[:32])), "little")
    return _encode(_scalarmult(_base(), a))

def _clamp(scalar: bytearray) -> bytes:
    scalar[0] &= 248
    scalar[31] &= 127
    scalar[31] |= 64
    return bytes(scalar)

def sign(seed: bytes, message: bytes) -> bytes:
    if len(seed) != 32:
        raise SystemExit("error: Ed25519 种子必须 32 字节")
    h = hashlib.sha512(seed).digest()
    a = int.from_bytes(_clamp(bytearray(h[:32])), "little")
    prefix = h[32:]
    r = _hint(prefix + message)
    r_encoded = _encode(_scalarmult(_base(), r))
    pk = _encode(_scalarmult(_base(), a))
    k = _hint(r_encoded + pk + message)
    s = (r + k * a) % L
    return r_encoded + s.to_bytes(32, "little")

def verify(pk: bytes, message: bytes, signature: bytes) -> bool:
    if len(pk) != 32 or len(signature) != 64:
        return False
    r_bytes, s_bytes = signature[:32], signature[32:]
    if int.from_bytes(s_bytes, "little") >= L:
        return False
    a = _decompress(pk)
    r_point = _decompress(r_bytes)
    if a is None or r_point is None:
        return False
    k = _hint(r_bytes + pk + message)
    left = _scalarmult(_base(), int.from_bytes(s_bytes, "little"))
    return _encode(left) == _encode(_add(r_point, _scalarmult(a, k)))



