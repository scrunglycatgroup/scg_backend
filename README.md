# SCG backend
![A picture of a waiter (in the style of a patent iamge](logo.png)
   | *Server software*

## How to set up

First, install poetry and a recent Python version.

To use poetry well, I would suggest setting up the virtual env. This requires running the commands:

```sh
poetry env use <your Python version>`
$(poetry env activate)
```

>   ### Note
>   On Windows, that second command may not work. Try just running:
>
>   ```sh
>   poetry env activate
>   ```
>
>   And then copy and paste the output, e.g. `source ~/.cache/pypoetry/virtualenvs/scg-backend-qOx8M4AN-py3.13/bin/activate` back into the Command Prompt and press enter.

Your current shell should now be using the correct Python environment!

Install all the project dependencies into this environment with:

```
poetry install
```

Ignore the error about the current project not being installed -- it has still installed
the dependencies.

To run the development server:

```
poetry run fastapi dev run.py
```

To run for production:

```
poetry run fastapi run run.py
```

## Getting docker 
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
