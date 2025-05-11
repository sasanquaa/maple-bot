import kmNet
import grpc

from concurrent import futures
# The two imports below is generated from:
# python -m grpc_tools.protoc --python_out=. --pyi_out=. --grpc_python_out=. -I../../backend/proto ../..
# /backend/proto/input.proto
from input_pb2 import Key, KeyRequest, KeyResponse
from input_pb2_grpc import KeyInputServicer, add_KeyInputServicer_to_server


class KeyInput(KeyInputServicer):
    def __init__(self, keys_map: dict[Key, int]) -> None:
        super().__init__()
        self.keys_map = keys_map

    def Send(self, request: KeyRequest, context):
        kmNet.keypress(self.keys_map[request.key], 0)
        return KeyResponse()

    def SendUp(self, request: KeyRequest, context):
        kmNet.keydown(self.keys_map[request.key])
        return KeyResponse()

    def SendDown(self, request: KeyRequest, context):
        kmNet.keyup(self.keys_map[request.key])
        return KeyResponse()


if __name__ == "__main__":
    kmNet.init("192.168.2.188", "8704", "33005C53")
    # Generated with ChatGPT, might not be accurate
    keys_map = {
        # Letters A-Z
        # A=0 -> HID 4, ..., Z=25 -> HID 29
        **{Key.Name(i): 4 + i for i in range(26)},

        # Digits 0–9
        Key.Zero: 39,
        Key.One: 30,
        Key.Two: 31,
        Key.Three: 32,
        Key.Four: 33,
        Key.Five: 34,
        Key.Six: 35,
        Key.Seven: 36,
        Key.Eight: 37,
        Key.Nine: 38,

        # Function keys
        Key.F1: 58,
        Key.F2: 59,
        Key.F3: 60,
        Key.F4: 61,
        Key.F5: 62,
        Key.F6: 63,
        Key.F7: 64,
        Key.F8: 65,
        Key.F9: 66,
        Key.F10: 67,
        Key.F11: 68,
        Key.F12: 69,

        # Arrows & navigation
        Key.Up: 82,
        Key.Down: 81,
        Key.Left: 80,
        Key.Right: 79,
        Key.Home: 74,
        Key.End: 77,
        Key.PageUp: 75,
        Key.PageDown: 78,
        Key.Insert: 73,
        Key.Delete: 76,

        # Modifiers and special characters
        Key.Ctrl: 224,
        Key.Enter: 40,
        Key.Space: 44,
        Key.Tilde: 53,
        Key.Quote: 52,
        Key.Semicolon: 51,
        Key.Comma: 54,
        Key.Period: 55,
        Key.Slash: 56,
        Key.Esc: 41,
        Key.Shift: 225,
        Key.Alt: 226,
    }

    server = grpc.server(futures.ThreadPoolExecutor(max_workers=1))
    add_KeyInputServicer_to_server(KeyInput(keys_map), server)
    server.add_insecure_port("[::]:5001")
    server.start()
    print("Server started, listening on 5001")
    server.wait_for_termination()
