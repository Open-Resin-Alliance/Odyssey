#!/usr/bin/env python3
import glob
import pygame
import os

mode = 0o600

real_bit_depth=[8]

bytes_per_pixelgroup=int(sum(real_bit_depth)/8)

fake_bit_depth=8


#sliced_x, sliced_y = (6480,3600)
#zoom_ratio=1

sliced_x, sliced_y = (192,108)
zoom_ratio=5


print(f"Rendering a screen {sliced_x*zoom_ratio}x{sliced_y*zoom_ratio}")
print(f"Models should be sliced for {sliced_x}x{sliced_y}, and a {zoom_ratio}x zoom will be applied")

print(f"Odyssey bit depths should be set to {real_bit_depth}, but will be rendered as 8 bit monochrome")

pygame.init()

surface = pygame.display.set_mode((sliced_x*zoom_ratio, sliced_y*zoom_ratio))

pixelArray = pygame.PixelArray(pygame.display.get_surface())

print(pygame.display.Info())


frame_size=int((sliced_x*sliced_y)*(bytes_per_pixelgroup/len(real_bit_depth)))
while True:
    fifo_path = glob.glob("/tmp/odysseyTest*/mockFb")[0]
    with open(fifo_path, mode='rb') as efb:
        efb.read()
        while True:
            print(f"file links: {os.fstat(efb.fileno()).st_nlink}")
            if os.fstat(efb.fileno()).st_nlink <= 0:
                print("File deleted")
                break
            elif efb.readable():
                data = b""

                while len(data)<frame_size:
                    data+=efb.read()

                print(f"Reading {len(data)} bytes from {fifo_path}")

                # convert into array of 8bit values
                final_data=[]

                grouped_data = [data[i:(i+bytes_per_pixelgroup)] for i in range(0, len(data), bytes_per_pixelgroup)]

                for group in grouped_data:
                    combined = int.from_bytes(group, byteorder="little")
                    #if (combined>0): print(f"{combined:>016b}")

                    pos_shift = sum(real_bit_depth)

                    for i in range(len(real_bit_depth)):
                        pos_shift -=real_bit_depth[i]

                        mask = ((1 << real_bit_depth[i]) -1) << pos_shift
                        single_pixel = (combined & mask) >> pos_shift

                        single_pixel = single_pixel << (fake_bit_depth-real_bit_depth[i])

                        final_data.append( single_pixel )

                for i in range(0, len(final_data)):
                    zoomed_x = (i*zoom_ratio)%(sliced_x*zoom_ratio)
                    zoomed_y = int((i)/(sliced_x))*zoom_ratio


                    for j in range(zoom_ratio):
                        for k in range(zoom_ratio):
                            pixelArray[zoomed_x+j, zoomed_y+k]=pygame.Color(final_data[i],final_data[i],final_data[i])

                    #if final_data[i]>0: print(f"{zoomed_x}, {zoomed_y}, {zoom_ratio}, {final_data[i]:>08b}")

                pygame.display.flip()
