## Customize Input
The bot currently does not use advanced input method such as a driver like `Interception` but only a normal Win32 API `SendInput`, so you should use at least be aware/cautious and use the bot default input mode at your own risk. If you want more security, customizing the bot with hardware input (KMBox, Arduino,...) using `Rpc` method provided in the `Settings` tab is recommended. However, this currently requires some scripting:
  - Use the language of your choice to write, host it and provide the server URL to the bot as long as you can generate gRPC stubs
  - Check this [example](https://github.com/sasanquaa/maple-bot/tree/master/examples/python)
