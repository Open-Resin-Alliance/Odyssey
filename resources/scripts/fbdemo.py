#!/usr/bin/env python3
import math
import time

frame_buffer = "/dev/fb0"

def bits_append(current,suffix, suffix_len):
    return (current << suffix_len) | suffix



def write_line(filepath, line, line_count, delay_ms=1):
    with open(filepath, mode="wb") as fb:
        for i in range(line_count):
            time.sleep(delay_ms/1000)
            fb.write(line)

def write_line_shifting(filepath,line,line_count,shift,delay_ms=1):
    with open(filepath,mode="wb") as fb:
        for i in range(line_count):
            time.sleep(delay_ms/1000)
            fb.write(line)
            for j in range(shift):
                line.insert(0,line.pop())

def write_lines(filepath,lines,delay_ms=1):
    with open(filepath, mode="wb") as fb:
        for line in lines:
            time.sleep(delay_ms/1000)
            fb.write(line)


def split_to_chunks(pixels, pixel_bit_len, pixels_per_chunk, chunk_bit_len, pad_end=True, pad_ones=True):
    assert chunk_bit_len%8==0,f"{chunk_bit_len}bit chunks cannot be split into whole bytes"
    assert pixel_bit_len*pixels_per_chunk<=chunk_bit_len, f"{pixels_per_chunk} pixels of {pixel_bit_len} bits do not fit in {chunk_bit_len}bit chunks"
    chunk_bytes = int(chunk_bit_len/8)
    pad_len = chunk_bit_len - (pixel_bit_len*pixels_per_chunk)

    out = bytearray()
    chunk = 0b0
    for i in range(len(pixels)):
        chunk = bits_append(chunk,pixels[i],pixel_bit_len)

        if (i+1)%pixels_per_chunk == 0:
            if pad_end:
                if pad_ones:
                    chunk = (chunk << pad_len) | ((1 << pad_len) -1)
                else:
                    chunk = chunk << pad_len
            else:
                if pad_ones:
                    chunk = chunk | ((1<<pad_len)-1) << (chunk_bit_len - pad_len)
            out.extend(chunk.to_bytes(chunk_bytes,"big"))
            chunk = 0b0
    return out

def pattern_line_bytes(pixel_vals, pixel_bit_len, total_pixel_count, pixels_per_chunk, chunk_bit_len, pad_end=True, pad_ones=True):
    assert total_pixel_count%len(pixel_vals)==0, f"Cannot divide {total_pixel_count}pixels into {len(pixel_vals)} values: {pixel_vals}"
    band_pixel_count = int(total_pixel_count/len(pixel_vals))
    pixels = [val for val in pixel_vals for i in range(band_pixel_count)]
    return split_to_chunks(pixels,pixel_bit_len, pixels_per_chunk, chunk_bit_len, pad_end=pad_end, pad_ones=pad_ones)






white_3b_left_pad = (0x00FFFFFF,32)
white_3b_right_pad = (0xFFFFFF00,32)
white_4b = (0xFFFFFFFF,32)

width_bytes = 7568
height = 6230
rgb_pixel_width = 1892
mono_pixel_width = 15136


pixel_vals = range(2**3)
bit_depth = 3

"""
while True:
    print(f"{[bin(i) for i in pixel_vals]} padded with 1s on the right")
    write_line(frame_buffer, pattern_line_bytes(pixel_vals, bit_depth,mono_pixel_width,8,32,True,True),height)
    print(f"{[bin(i) for i in pixel_vals]} padded with 0s on the right")
    write_line(frame_buffer, pattern_line_bytes(pixel_vals, bit_depth,mono_pixel_width,8,32,True,False),height)

    print(f"{[bin(i) for i in pixel_vals]} padded with 1s on the left")
    write_line(frame_buffer, pattern_line_bytes(pixel_vals, bit_depth,mono_pixel_width,8,32,False,True),height)

    print(f"{[bin(i) for i in pixel_vals]} padded with 0s on the left")
    write_line(frame_buffer, pattern_line_bytes(pixel_vals, bit_depth,mono_pixel_width,8,32,False,False),height)
"""
pattern = pattern_line_bytes([0,2**3-1,]*16,bit_depth, mono_pixel_width, 8, 32)

#while True:
write_line_shifting(frame_buffer,pattern, height,4,delay_ms=0.5)
    #pattern.insert(0,pattern.pop())
    #pattern.insert(0,pattern.pop())
    #pattern.insert(0,pattern.pop())
    #pattern.insert(0,pattern.pop())


