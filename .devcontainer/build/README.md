The build folder is used to build the HermitGrab DevContainer. It is build using:
```sh
devcontainer build --platform=linux/arm64,linux/amd64 --image-name=ghcr.io/karstenb/hermitgrab-devcontainer --label org.opencontainers.image.source=https://github.com/KarstenB/HermitGrab --workspace-folder=$PWD/ --config ./.devcontainer/build/devcontainer.json
```