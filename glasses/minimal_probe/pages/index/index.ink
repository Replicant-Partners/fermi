<script type="application/json" def>
{
  "navigationBarTitleText": "Probe"
}
</script>

<script setup>
export default {
  data: {
    message: 'probe ok',
  },
};
</script>

<page>
  <view class="box">
    <text class="line">{{ message }}</text>
  </view>
</page>

<style>
.box {
  width: 480px;
  min-height: 120px;
  background: #000000;
  padding: 16px;
}

.line {
  font-family: monospace;
  font-size: 18px;
  color: #40ff5e;
}
</style>
