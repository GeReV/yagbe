﻿# Yet Another GameBoy Emulator

A simple GameBoy emulator in Rust, built as a practice project.

It was created as an attempt at building a working emulator based almost entirely on documentation on the console.

While writing the code, I tried to avoid reading code examples and sources of other emulators, as a soft-limitation on myself. 
I did resort to reading examples, however, on a few non-GB specific things, like some CPU instruction behaviors, especially with respect to the CPU flags.

Emulators with built-in debuggers were also used as reference.

The project makes use of very few external dependencies. The only used crates are the [Rust SDL2 bindings](https://github.com/Rust-SDL2/rust-sdl2) 
and the [bitflags crate](https://docs.rs/bitflags/latest/bitflags/) for convenience.

## Resources used

Most resources used were picked up in the excellent [Awesome Game Boy Development](https://github.com/gbdev/awesome-gbdev) list on GitHub.

Before starting the project, I read the "[Writing NES Emulator in Rust](https://bugzmanov.github.io/nes_ebook/)" e-book 
by [@bugzmanov](https://github.com/bugzmanov/), to get a general idea of how an emulator is structured.

#### Reference:
- [Pan Docs](https://gbdev.github.io/pandocs/) - Used as the main reference for the entire console.
- [Pan Docs Rendering Internals](https://github.com/gbdev/pandocs/blob/bbdc0ef79ba46dcc8183ad788b651ae25b52091d/src/Rendering_Internals.md) - No longer included in the full docs, but clarifies a few things about rendering.
- [The Ultimate Game Boy Talk](https://media.ccc.de/v/33c3-8029-the_ultimate_game_boy_talk) by [Michael Steil](https://github.com/mist64) - Used mainly as another reference to understanding the Game Boy rendering.

#### CPU
- [gb-opcodes](https://gbdev.github.io/gb-opcodes/optables/) - Table of CPU instructions, arranged by their hex representation.
- [RGBDS opcodes reference](https://rgbds.gbdev.io/docs/gbz80.7) - A more detailed reference of the opcodes.

#### Debugging
- [BGB](https://bgb.bircd.org/) - An emulator with a visual debugger.
- [Emulicious](https://emulicious.net/) - Another emulator with a very powerful built-in debugger.

#### Testing
- [Blargg's test roms](http://gbdev.gg8.se/files/roms/blargg-gb-tests/) - Used to test the CPU implementation.
- [dmg-acid2](https://github.com/mattcurrie/dmg-acid2) - Used to test the image rendering.

## Notes

1. Debugging the test roms proved a bit challenging when trying to find how and when my implementation was digressing
   from the expected behavior by simply using a debugger.

   Using [Emulicious'](https://emulicious.net/) Trace Logger helped tremendously, by saving a log of Emulicious 
   passing the tests correctly and comparing it to a matching log generated by my code using a simple text diffing tool.
   
   The diff helped pinpoint the exact instructions that contained bugs in their implementations.
2. Spent a few days and multiple approaches trying to write the rendering code. The Pan Docs documentation was confusing,
   and the process difficult to debug.
   
   After multiple nights of scratching my head, searching the internet, fiddling with off-by-one errors and reading the 
   docs over and over, the emulator finally rendered the correct reference image of the dmg-acid2 test.
   
   Ironically, any game ROMs tested still failed to show an image. As of writing these lines, I believe this is due to
   the audio system being unimplemented.
3. After debugging the emulator's run on a ROM of [Tetris](https://en.wikipedia.org/wiki/Tetris_(Game_Boy_video_game)) 
   and comparing it to against traces from Emulicious, it turned out that the implementation of the joystick inputs was
   not properly implemented.  
   
   After fixing those issues, the emulator ran its first game roms with at least some success.
4. Eventually rewrote the rendering subsystem to something that's closer to how the device works, 
   using the Rendering Internals page (linked above) of the Pan Docs. 
   
   Since some things were still unclear, I went to watch the rendering part in The Ultimate Game Boy Talk (linked above).
   
   While it clarified even more bits about the topic, it seems more changes are required.
   