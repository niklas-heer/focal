# focal

> Common development tasks for the Electron, React, and Vite app.

## install

> Install project dependencies.

```sh
npm install
```

## dev

> Start the Vite dev server and Electron app.

```sh
npm run dev
```

## build

> Compile the Electron process and bundle the renderer.

```sh
npm run build
```

## preview

> Preview the production renderer bundle locally.

```sh
npm run preview
```

## lint

> Run the TypeScript type-check.

```sh
npm run lint
```

## test-scroll-render

> Run the Playwright smoke test for markdown decoration persistence while scrolling.

```sh
npm run test:scroll-render
```

## package

> Build a distributable app with electron-builder.

```sh
npx electron-builder
```

## clean

> Remove generated build output directories.

```sh
rm -rf dist electron-dist
```
