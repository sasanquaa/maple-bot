from concurrent import futures
from pywinauto import WindowSpecification, keyboard
from pywinauto.application import Application

import grpc
import pywinauto
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
            keyboard.send_keys("{" + self.keys_map[request.key] + "}", pause=0)
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
    # Add more mappings
    keys_map = {
        Key.Space: 'VK_SPACE',
        Key.Up: 'UP',
        Key.Down: 'DOWN',
        Key.Left: 'LEFT',
        Key.Right: 'RIGHT',
    }

    server = grpc.server(futures.ThreadPoolExecutor(max_workers=10))
    add_KeyInputServicer_to_server(KeyInput(window, keys_map), server)
    server.add_insecure_port("[::]:5001")
    server.start()
    print("Server started, listening on 5001")
    server.wait_for_termination()
