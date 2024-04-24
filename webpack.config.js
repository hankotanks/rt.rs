const path = require('path');
const CopyPlugin = require('copy-webpack-plugin');
const WasmPackPlugin = require('@wasm-tool/wasm-pack-plugin');
const TerserPlugin = require('terser-webpack-plugin');

const dist = path.resolve(__dirname, 'dist');

const publicPath = process.env.DEPLOY_PAGES ? '/rt.rs/' : undefined;

module.exports = {
    mode: 'production',
    entry: {
        index: './js/index.js'
    },
    output: {
        path: dist,
        filename: '[name].js',
        publicPath
    },
    devServer: {
        contentBase: dist,
    },
    plugins: [
        new CopyPlugin([
            path.resolve(__dirname, 'static')
        ]),
        new WasmPackPlugin({
            crateDirectory: __dirname,
        }),
    ],
    optimization: {
        minimize: true,
        minimizer: [
            new TerserPlugin({
                terserOptions: {
                    ecma: undefined,
                    parse: {},
                    compress: {},
                    mangle: true,
                    module: true,
                },
            }),
        ],
    }
};
