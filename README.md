# SCG backend
![A picture of a waiter (in the style of a patent image](logo.png)
   | *Server software*

## How to setup

You are going to need poetry
To use poetry well I would suggest setting up the virtual env this requires running the commands
`poetry env use 3.13.2`
`poetry env activate`
then run the command that it shows i.e.
`source ~/.cache/pypoetry/virtualenvs/scg-backend-qOx8M4AN-py3.13/bin/activate`

alternatively you can use
```
poetry env activate | ``
```
which just runs whatever it pastes

`poetry install --no-root` will install all the dependencies

currently running debug goes : `poetry run fastapi dev run.py`

### Getting docker 
You will need docker to run this

   TODO: write docker install

### Using docker
Currently I have the kafka service and database on docker!
which means we need to run them both, i use (at the moment)...

```
docker run -d --name=kafka -p 9092:9092 apache/kafka 
```
and
```

docker run -d --rm --name=database- pull always -p 8008:8000 surrealdb/surrealdb:latest-dev start --user root --pass root
```

### Now the services
Start with the python application using
```
poetry run fastapi run run.py
```

and then start the rust app with

```
cargo run --release
```

This is a lot of stuff, I need to write a docker compose, also rewrite the entirety of the rust code because it is awful, maybe everything as well, things need command line arguments for what addresses they should point at for both kafka and the database, and generally everything should be given a strong lick of paint

## Plans
### Current Goals
- [ ] HTTP REST API (in progress)
- [ ] LLM Code (in progress)
- [ ] Document the whole thing
- [x] Containerize it (in progress)
- [ ] Anything else to get it to work best on either Viking or personal computers
