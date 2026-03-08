// Rebuild node-pty against Electron headers using the JS API
// (electron-rebuild CLI fails on Node 25 due to ESM/CJS yargs conflict)
const path = require('path')
const electronVersion = require(path.join(__dirname, '../node_modules/electron/package.json')).version

require(path.join(__dirname, '../node_modules/electron-rebuild'))
  .rebuild({
    buildPath: path.join(__dirname, '..'),
    electronVersion,
    force: true,
    onlyModules: ['node-pty'],
  })
  .then(() => {
    console.log('node-pty rebuilt successfully')
  })
  .catch((err) => {
    console.error('Failed to rebuild node-pty:', err.message)
    process.exit(1)
  })
