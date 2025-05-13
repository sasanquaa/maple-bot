import pywinauto
import grpc

from concurrent import futures
from pywinauto import WindowSpecification, keyboard
from pywinauto.application import Application
# The two imports below is generated from:
# python -m grpc_tools.protoc --python_out=. --pyi_out=. --grpc_python_out=. -I../../backend/proto ../..
# /backend/proto/input.proto
from input_pb2 import Key, KeyRequest, KeyResponse
from input_pb2_grpc import KeyInputServicer, add_KeyInputServicer_to_server


class KeyInput(KeyInputServicer):
    def __init__(self, window: WindowSpecification, keys_map: dict[Key, str]) -> None:
        super().__init__()
        self.window = window
        self.keys_map = keys_map

    def Send(self, request: KeyRequest, context):
        if self.window.has_keyboard_focus():
            key = self.keys_map[request.key]
            if len(key) == 1:
                keyboard.send_keys(self.keys_map[request.key], pause=0)
            else:
                keyboard.send_keys(
                    "{" + self.keys_map[request.key] + "}", pause=0)
        return KeyResponse()

    def SendUp(self, request: KeyRequest, context):
        if self.window.has_keyboard_focus():
            keyboard.send_keys(
                "{" + self.keys_map[request.key] + " up}", pause=0)
        return KeyResponse()

    def SendDown(self, request: KeyRequest, context):
        if self.window.has_keyboard_focus():
            keyboard.send_keys(
                "{" + self.keys_map[request.key] + " down}", pause=0)
        return KeyResponse()


if __name__ == "__main__":
    window_args = {'class_name': 'MapleStoryClass'}
    window = Application().connect(
        handle=pywinauto.findwindows.find_window(
            **window_args)).window()
    # Generated with ChatGPT, might not be accurate
    keys_map = {
        # Letters
        Key.A: 'a',
        Key.B: 'b',
        Key.C: 'c',
        Key.D: 'd',
        Key.E: 'e',
        Key.F: 'f',
        Key.G: 'g',
        Key.H: 'h',
        Key.I: 'i',
        Key.J: 'j',
        Key.K: 'k',
        Key.L: 'l',
        Key.M: 'm',
        Key.N: 'n',
        Key.O: 'o',
        Key.P: 'p',
        Key.Q: 'q',
        Key.R: 'r',
        Key.S: 's',
        Key.T: 't',
        Key.U: 'u',
        Key.V: 'v',
        Key.W: 'w',
        Key.X: 'x',
        Key.Y: 'y',
        Key.Z: 'z',

        # Digits
        Key.Zero: '0',
        Key.One: '1',
        Key.Two: '2',
        Key.Three: '3',
        Key.Four: '4',
        Key.Five: '5',
        Key.Six: '6',
        Key.Seven: '7',
        Key.Eight: '8',
        Key.Nine: '9',

        # Function Keys
        Key.F1: 'F1',
        Key.F2: 'F2',
        Key.F3: 'F3',
        Key.F4: 'F4',
        Key.F5: 'F5',
        Key.F6: 'F6',
        Key.F7: 'F7',
        Key.F8: 'F8',
        Key.F9: 'F9',
        Key.F10: 'F10',
        Key.F11: 'F11',
        Key.F12: 'F12',

        # Navigation and Controls
        Key.Up: 'UP',
        Key.Down: 'DOWN',
        Key.Left: 'LEFT',
        Key.Right: 'RIGHT',
        Key.Home: 'HOME',
        Key.End: 'END',
        Key.PageUp: 'PGUP',
        Key.PageDown: 'PGDN',
        Key.Insert: 'INSERT',
        Key.Delete: 'DEL',
        Key.Esc: 'ESC',
        Key.Enter: 'ENTER',
        Key.Space: 'SPACE',

        # Modifier Keys
        Key.Ctrl: '^',   # control (can also be '{VK_CONTROL}' if needed)
        Key.Shift: '+',  # shift (can also be '{VK_SHIFT}')
        Key.Alt: '%',    # alt (can also be '{VK_MENU}')

        # Punctuation & Special Characters
        Key.Tilde: '`',
        Key.Quote: "'",
        Key.Semicolon: ';',
        Key.Comma: ',',
        Key.Period: '.',
        Key.Slash: '/',
    }

    server = grpc.server(futures.ThreadPoolExecutor(max_workers=1))
    add_KeyInputServicer_to_server(KeyInput(window, keys_map), server)
    server.add_insecure_port("[::]:5001")
    server.start()
    print("Server started, listening on 5001")
    server.wait_for_termination()
