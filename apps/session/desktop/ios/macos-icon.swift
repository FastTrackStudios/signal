// Turn the full-bleed iOS icon master into a macOS one: macOS does not mask
// app icons, so the art goes on Apple's icon grid itself — an 824pt rounded
// tile centred on a transparent 1024 canvas, with a soft drop shadow.
//   swift macos-icon.swift <icon-1024.png> <out.png>
import CoreGraphics
import Foundation
import ImageIO

let args = CommandLine.arguments
let src = CGImageSourceCreateWithURL(URL(fileURLWithPath: args[1]) as CFURL, nil)!
let icon = CGImageSourceCreateImageAtIndex(src, 0, nil)!
let ctx = CGContext(
    data: nil, width: 1024, height: 1024, bitsPerComponent: 8, bytesPerRow: 0,
    space: CGColorSpace(name: CGColorSpace.sRGB)!,
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
let tile = CGRect(x: 100, y: 100, width: 824, height: 824)
let shape = CGPath(roundedRect: tile, cornerWidth: 185, cornerHeight: 185, transform: nil)

ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -10), blur: 28,
              color: CGColor(gray: 0, alpha: 0.35))
ctx.addPath(shape)
ctx.setFillColor(CGColor(gray: 0.05, alpha: 1))
ctx.fillPath()
ctx.restoreGState()

ctx.addPath(shape)
ctx.clip()
ctx.interpolationQuality = .high
ctx.draw(icon, in: tile)

let dst = CGImageDestinationCreateWithURL(
    URL(fileURLWithPath: args[2]) as CFURL, "public.png" as CFString, 1, nil)!
CGImageDestinationAddImage(dst, ctx.makeImage()!, nil)
guard CGImageDestinationFinalize(dst) else { exit(1) }
