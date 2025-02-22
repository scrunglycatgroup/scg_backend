# SCG backend
![A picture of a waiter (in the style of a patent iamge](logo.png)
   | *Server software*

## How to setup

### Getting docker 
You will need docker to run this TODO: write docker install

### Next you will need to build the image

```sh
docker build -t scg_backend_image .
```
Will build 2 images, one is the build image (where the binary is compiled), the other is the runner image (a minimal ubuntu setup for actually running the program)

The compiled image should be distributable to other machines in docker it will have the name "scg_backend_image"

### Running the image
Currently I run the image with:
```sh
docker run --network host -d -it -p 8080:80 --rm --name scg_runner scg_backend_image:latest
```

### Test that it is running
you should be able to open 0.0.0.0:8080 to get a "hello world!" message

## Plans
### Current Goals
- [ ] HTTP REST API (in progress)
- [ ] LLM Code (in progress)
- [ ] Document the whole thing
- [x] Containerize it (in progress)
- [ ] Anything else to get it to work best on either Viking or personal computers

This is still very very in production, so lots to do just getting something out asap will work on this a lot soon!
This is where I am getting the basis for some of the code for running the NN 
`https://github.com/huggingface/candle/blob/main/candle-examples/examples/qwen/main.rs`
